import * as anchor from "@anchor-lang/core";
import { IdlTypes, Program } from "@anchor-lang/core";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  TransactionInstruction,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  ExtensionType,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  getMintLen,
  createSetAuthorityInstruction,
  AuthorityType,
} from "@solana/spl-token";
import {
  TestSetup,
  PDAUtils,
  InstructionHelper,
  installStubHookProgram,
  timeTravelTo,
  chainTime,
  setAccountData,
  createMintPair,
  getValidTickForLimitOrder,
} from "./utils";
import {
  getSqrtPriceX64FromTick,
  getTickArrayStartIndex,
  MEMO_PROGRAM_ID,
} from "./utils/tick-math";
import { assert } from "chai";
import { ComputeBudgetProgram, type AccountMeta } from "@solana/web3.js";

const STUB_ID = new PublicKey("6s7wXukDLKt39UKgbQVExCpwrAsRraEB8jkV6P95ZcSR");
// sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]
const INIT_LIST_DISCRIMINATOR = Buffer.from([43, 34, 13, 49, 167, 88, 235, 235]);
// `AccountsType` variants, see util/remaining_accounts.rs
type AccountsType = IdlTypes<RaydiumClmm>["accountsType"];
const TICK_ARRAY: AccountsType = { tickArray: {} };
const TRANSFER_HOOK_INPUT: AccountsType = { transferHookInput: {} };
const TRANSFER_HOOK_OUTPUT: AccountsType = { transferHookOutput: {} };
const TICK_ARRAY_BITMAP_EXTENSION: AccountsType = { tickArrayBitmapExtension: {} };
const TRANSFER_HOOK_TOKEN_0: AccountsType = { transferHookToken0: {} };
const TRANSFER_HOOK_TOKEN_1: AccountsType = { transferHookToken1: {} };
const TRANSFER_HOOK_REWARD_0: AccountsType = { transferHookReward0: {} };
const REWARD_GROUP: AccountsType = { rewardGroup: {} };
const MINT_ADMISSION: AccountsType = { mintAdmission: {} };

describe("transfer hook pool admission", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.RaydiumClmm as Program<RaydiumClmm>;
  const connection = provider.connection;
  const owner = provider.wallet.payer;
  const setup = new TestSetup(program, owner);
  const pda = new PDAUtils(program.programId);
  const instructions = new InstructionHelper(program);

  before(async () => {
    await installStubHookProgram(
      connection,
      STUB_ID,
      "target/deploy/transfer_hook_stub.so",
      owner.publicKey
    );
    await setup.initialize();
    await setup.createTokens();
    await setup.mintTokens();
    await setup.createAmmConfig(0);
  });

  /// Admission for an external hook mint is the `SupportMintAssociated` whitelist; the list
  /// checks that follow only decide whether the mint is *tradable*.
  async function whitelist(mint: PublicKey): Promise<PublicKey> {
    const [supportMint] = PublicKey.findProgramAddressSync(
      [Buffer.from("support_mint"), mint.toBuffer()],
      program.programId
    );
    await program.methods
      .createSupportMintAssociated()
      .accounts({
        owner: owner.publicKey,
        tokenMint: mint,
        supportMintAssociated: supportMint,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([owner])
      .rpc();
    return supportMint;
  }

  /// An extra account taken from the destination token account's owner field. On a deposit
  /// into a vault it resolves to the pool PDA.
  const DESTINATION_OWNER_META = (() => {
    const meta = Buffer.alloc(35);
    meta[0] = 2; // PubkeyData
    meta.set([2, 2, 32], 1); // AccountData { account_index: 2 (destination), data_index: 32 }
    return meta;
  })();

  function listAddress(mint: PublicKey): PublicKey {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("extra-account-metas"), mint.toBuffer()],
      STUB_ID
    )[0];
  }

  /// A token-2022 mint pointing at the stub hook.
  async function createHookMint(): Promise<PublicKey> {
    const mintKeypair = Keypair.generate();
    const space = getMintLen([ExtensionType.TransferHook]);
    const lamports = await connection.getMinimumBalanceForRentExemption(space);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        SystemProgram.createAccount({
          fromPubkey: owner.publicKey,
          newAccountPubkey: mintKeypair.publicKey,
          space,
          lamports,
          programId: TOKEN_2022_PROGRAM_ID,
        }),
        createInitializeTransferHookInstruction(
          mintKeypair.publicKey,
          owner.publicKey,
          STUB_ID,
          TOKEN_2022_PROGRAM_ID
        ),
        createInitializeMintInstruction(
          mintKeypair.publicKey,
          9,
          owner.publicKey,
          null,
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner, mintKeypair]
    );
    return mintKeypair.publicKey;
  }

  /// Create the mint's `ExtraAccountMetaList` (empty) through the stub, the way a real hook
  /// program would: the caller funds the PDA first, the program only allocates and writes.
  async function createEmptyList(mint: PublicKey, metas: Buffer[] = []): Promise<PublicKey> {
    const list = listAddress(mint);
    const count = Buffer.alloc(4);
    count.writeUInt32LE(metas.length);
    const data = Buffer.concat([INIT_LIST_DISCRIMINATOR, count, ...metas]);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        SystemProgram.transfer({
          fromPubkey: owner.publicKey,
          toPubkey: list,
          lamports: await connection.getMinimumBalanceForRentExemption(256),
        }),
        new TransactionInstruction({
          programId: STUB_ID,
          keys: [
            { pubkey: list, isSigner: false, isWritable: true },
            { pubkey: mint, isSigner: false, isWritable: false },
            { pubkey: owner.publicKey, isSigner: true, isWritable: false },
            {
              pubkey: SystemProgram.programId,
              isSigner: false,
              isWritable: false,
            },
          ],
          data,
        })
      ),
      [owner]
    );
    return list;
  }

  /// A literal `ExtraAccountMeta`: read-only, non-signer.
  function literalMeta(key: PublicKey): Buffer {
    const meta = Buffer.alloc(35);
    key.toBuffer().copy(meta, 1);
    return meta;
  }

  /// Rewrite a list in place, as its hook program would after the pool exists.
  /// TLV: type (8) | length (4) | count (4) | metas.
  async function rewriteList(list: PublicKey, metas: Buffer[]) {
    const current = (await connection.getAccountInfo(list))!.data;
    const value = Buffer.concat([Buffer.alloc(4), ...metas]);
    value.writeUInt32LE(metas.length, 0);
    const length = Buffer.alloc(4);
    length.writeUInt32LE(value.length);
    await setAccountData(connection, list, Buffer.concat([current.subarray(0, 8), length, value]));
  }

  async function createPool(
    hookMint: PublicKey,
    extra: { list?: boolean; whitelist?: PublicKey; plain?: PublicKey } = {}
  ) {
    const plain = extra.plain ?? setup.token0;
    const hookFirst = hookMint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [hookMint, plain] : [plain, hookMint];
    const [program0, program1] = hookFirst
      ? [TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID]
      : [TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID];

    const [poolState] = await pda.getPoolStatePDA(setup.ammConfig, mint0, mint1);
    const [tokenVault0] = await pda.getTokenVaultPDA(poolState, mint0);
    const [tokenVault1] = await pda.getTokenVaultPDA(poolState, mint1);
    const [observationState] = await pda.getObservationStatePDA(poolState);
    const [tickArrayBitmap] = await pda.getTickArrayBitmapPDA(poolState);

    return await program.methods
      .createPool(
        getSqrtPriceX64FromTick(0),
        new anchor.BN(Math.floor(Date.now() / 1000) - 100)
      )
      .accounts({
        poolCreator: owner.publicKey,
        ammConfig: setup.ammConfig,
        poolState,
        tokenMint0: mint0,
        tokenMint1: mint1,
        tokenVault0,
        tokenVault1,
        observationState,
        tickArrayBitmap,
        tokenProgram0: program0,
        tokenProgram1: program1,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .remainingAccounts([
        ...(extra.whitelist
          ? [{ pubkey: extra.whitelist, isSigner: false, isWritable: false }]
          : []),
        ...(extra.list
          ? [
              {
                pubkey: listAddress(hookMint),
                isSigner: false,
                isWritable: false,
              },
            ]
          : []),
      ])
      .signers([owner])
      .rpc();
  }

  it("creates a pool for a hook mint without the whitelist", async () => {
    const mint = await createHookMint();
    await createEmptyList(mint);
    await createPool(mint, { list: true });
  });

  it("creates a pool for a hook mint whose ExtraAccountMetaList does not exist yet", async () => {
    const mint = await createHookMint();
    await createPool(mint);
  });

  it("creates a pool for a whitelisted hook mint once the list exists", async () => {
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    const list = await createEmptyList(mint);

    const listAccount = await connection.getAccountInfo(list);
    assert.isNotNull(listAccount);
    assert.equal(listAccount!.owner.toBase58(), STUB_ID.toBase58());

    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [poolState] = await pda.getPoolStatePDA(
      setup.ammConfig,
      hookFirst ? mint : plain,
      hookFirst ? plain : mint
    );
    const pool = await program.account.poolState.fetch(poolState);
    assert.equal(
      (hookFirst ? pool.tokenMint0 : pool.tokenMint1).toBase58(),
      mint.toBase58()
    );
  });

  it("refuses a legacy instruction on a hook pool", async () => {
    // The v1/v2 account layouts have nowhere to put a hook's accounts, so they reject the mint
    // outright rather than failing deep inside token-2022. Thirteen instructions share this
    // guard; `swap_v2` stands in for all of them.
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    await createEmptyList(mint);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [poolState] = await pda.getPoolStatePDA(
      setup.ammConfig,
      hookFirst ? mint : plain,
      hookFirst ? plain : mint
    );

    // Anchor validates accounts before the handler runs, so the swapper needs an account for
    // the hook mint even though the instruction is going to refuse the mint anyway.
    const outputAta = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          outputAta,
          owner.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );

    let err: any;
    try {
      await instructions.swapV2({
        owner,
        ammConfig: setup.ammConfig,
        poolState,
        inputVaultMint: plain,
        outputVaultMint: mint,
        amount: new anchor.BN(1000),
        otherAmountThreshold: new anchor.BN(0),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: true,
        remainingAccounts: [],
        outputTokenProgram: TOKEN_2022_PROGRAM_ID,
      });
    } catch (e) {
      err = e;
    }
    assert.isDefined(err, "swap_v2 accepted a hook pool");
    assert.include(
      JSON.stringify(err),
      "UnsupportedTransferHookMint",
      `unexpected: ${err}`
    );
  });

  it("opens a position and swaps on a hook pool through the v3 instructions", async () => {
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    await createEmptyList(mint);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [mint, plain] : [plain, mint];
    const [program0, program1] = hookFirst
      ? [TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID]
      : [TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID];
    const [poolState] = await pda.getPoolStatePDA(setup.ammConfig, mint0, mint1);

    // The hook mint has no supply yet and the position owner needs both sides.
    const hookAta = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          hookAta,
          owner.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(
          mint,
          hookAta,
          owner.publicKey,
          1_000_000_000,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );

    // The bag for whichever leg is the hook mint; the other leg declares nothing.
    const bag = [
      { pubkey: STUB_ID, isSigner: false, isWritable: false },
      { pubkey: listAddress(mint), isSigner: false, isWritable: false },
    ];

    const position = await instructions.openPositionV3({
      payer: owner,
      positionNftOwner: owner.publicKey,
      poolState,
      tickLowerIndex: -120,
      tickUpperIndex: 120,
      liquidity: new anchor.BN(1_000_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      tokenVault0Mint: mint0,
      tokenVault1Mint: mint1,
      token0Program: program0,
      token1Program: program1,
      remainingAccountsInfo: {
        slices: [
          {
            accountsType: hookFirst ? TRANSFER_HOOK_TOKEN_0 : TRANSFER_HOOK_TOKEN_1,
            length: bag.length,
          },
        ],
      },
      remainingAccounts: bag,
    });
    assert.isDefined(position.personalPosition);

    const poolAfterOpen = await program.account.poolState.fetch(poolState);
    assert.isTrue(
      poolAfterOpen.liquidity.gtn(0),
      "open_position_v3 added no liquidity"
    );

    // Plain mint in, hook mint out. Hand over the array holding the current tick plus the one
    // below it - a zero-for-one swap walks downwards - and declare exactly that on the tape,
    // which `parse_remaining_accounts` checks for exact consumption.
    const spacing = poolAfterOpen.tickSpacing;
    const ticksPerArray = spacing * 60;
    const currentStart = getTickArrayStartIndex(
      poolAfterOpen.tickCurrent,
      spacing
    );
    const tickArrays: AccountMeta[] = [];
    for (const start of [currentStart, currentStart - ticksPerArray]) {
      const [addr] = await pda.getTickArrayStatePDA(poolState, start);
      tickArrays.push({ pubkey: addr, isSigner: false, isWritable: true });
    }
    const swapAccounts = [...tickArrays, ...bag];
    await instructions.swapV3({
      owner,
      ammConfig: setup.ammConfig,
      poolState,
      inputVaultMint: plain,
      outputVaultMint: mint,
      amount: new anchor.BN(1000),
      otherAmountThreshold: new anchor.BN(0),
      sqrtPriceLimitX64: new anchor.BN(0),
      isBaseInput: true,
      outputTokenProgram: TOKEN_2022_PROGRAM_ID,
      remainingAccountsInfo: {
        slices: [
          { accountsType: TICK_ARRAY, length: tickArrays.length },
          { accountsType: TRANSFER_HOOK_OUTPUT, length: bag.length },
        ],
      },
      remainingAccounts: swapAccounts,
    });

    const poolAfterSwap = await program.account.poolState.fetch(poolState);
    assert.isFalse(
      poolAfterSwap.sqrtPriceX64.eq(poolAfterOpen.sqrtPriceX64),
      "swap_v3 did not move the price"
    );

    // Both liquidity legs carry the hook bag on whichever side the hook mint landed.
    const liquidityTape = {
      slices: [
        {
          accountsType: hookFirst ? TRANSFER_HOOK_TOKEN_0 : TRANSFER_HOOK_TOKEN_1,
          length: bag.length,
        },
      ],
    };
    const liquidityArgs = {
      owner,
      position,
      mint0,
      mint1,
      token0Program: program0,
      token1Program: program1,
      remainingAccountsInfo: liquidityTape,
      remainingAccounts: bag,
    };

    await instructions.increaseLiquidityV3({
      ...liquidityArgs,
      liquidity: new anchor.BN(500_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
    });
    const afterIncrease = await program.account.personalPositionState.fetch(
      position.personalPosition
    );
    assert.isTrue(
      afterIncrease.liquidity.gt(new anchor.BN(1_000_000)),
      "increase_liquidity_v3 added nothing"
    );

    await instructions.decreaseLiquidityV3({
      ...liquidityArgs,
      liquidity: new anchor.BN(500_000),
      amount0Min: new anchor.BN(0),
      amount1Min: new anchor.BN(0),
    });
    const afterDecrease = await program.account.personalPositionState.fetch(
      position.personalPosition
    );
    assert.isTrue(
      afterDecrease.liquidity.lt(afterIncrease.liquidity),
      "decrease_liquidity_v3 removed nothing"
    );
  });

  it("fails a swap that leaves out any of the hook accounts", async () => {
    // The stub checks what it receives like a real hook does, so this is what proves the
    // happy-path tests actually forward the list and its extras rather than just the program.
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    const extra = Keypair.generate().publicKey;
    const literal = Buffer.alloc(35); // discriminator 0, address, not signer, not writable
    extra.toBuffer().copy(literal, 1);
    await createEmptyList(mint, [literal]);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [mint, plain] : [plain, mint];
    const [program0, program1] = hookFirst
      ? [TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID]
      : [TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID];
    const [poolState] = await pda.getPoolStatePDA(
      setup.ammConfig,
      mint0,
      mint1
    );

    const hookAta = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          hookAta,
          owner.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(
          mint,
          hookAta,
          owner.publicKey,
          1_000_000_000,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );

    const asMetas = (keys: PublicKey[]): AccountMeta[] =>
      keys.map((pubkey) => ({ pubkey, isSigner: false, isWritable: false }));
    const fullBag = asMetas([STUB_ID, listAddress(mint), extra]);

    await instructions.openPositionV3({
      payer: owner,
      positionNftOwner: owner.publicKey,
      poolState,
      tickLowerIndex: -120,
      tickUpperIndex: 120,
      liquidity: new anchor.BN(1_000_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      tokenVault0Mint: mint0,
      tokenVault1Mint: mint1,
      token0Program: program0,
      token1Program: program1,
      remainingAccountsInfo: {
        slices: [
          {
            accountsType: hookFirst
              ? TRANSFER_HOOK_TOKEN_0
              : TRANSFER_HOOK_TOKEN_1,
            length: fullBag.length,
          },
        ],
      },
      remainingAccounts: fullBag,
    });

    const pool = await program.account.poolState.fetch(poolState);
    const ticksPerArray = pool.tickSpacing * 60;
    const currentStart = getTickArrayStartIndex(
      pool.tickCurrent,
      pool.tickSpacing
    );
    const tickArrays: AccountMeta[] = [];
    for (const start of [currentStart, currentStart - ticksPerArray]) {
      const [addr] = await pda.getTickArrayStatePDA(poolState, start);
      tickArrays.push({ pubkey: addr, isSigner: false, isWritable: true });
    }

    // Plain mint in, hook mint out.
    const swapWith = (keys: PublicKey[]) =>
      instructions.swapV3({
        owner,
        ammConfig: setup.ammConfig,
        poolState,
        inputVaultMint: plain,
        outputVaultMint: mint,
        amount: new anchor.BN(1000),
        otherAmountThreshold: new anchor.BN(0),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: true,
        outputTokenProgram: TOKEN_2022_PROGRAM_ID,
        remainingAccountsInfo: {
          slices: [
            { accountsType: TICK_ARRAY, length: tickArrays.length },
            { accountsType: TRANSFER_HOOK_OUTPUT, length: keys.length },
          ],
        },
        remainingAccounts: [...tickArrays, ...asMetas(keys)],
      });

    // Without the list the interface helpers silently forward nothing but the program, and
    // token-2022 would call the hook with the four transfer accounts only. The program has to
    // refuse that itself rather than trust every hook to notice.
    let err: any;
    try {
      await swapWith([STUB_ID, extra]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(
      err,
      "a swap without the ExtraAccountMetaList went through"
    );
    assert.include(
      JSON.stringify(err),
      "NoExtraAccountsForTransferHook",
      `unexpected: ${err}`
    );

    // Without an extra, resolution fails inside the program before token-2022 is reached.
    err = undefined;
    try {
      await swapWith([STUB_ID, listAddress(mint)]);
    } catch (e) {
      err = e;
    }
    assert.isDefined(
      err,
      "a swap without the declared extra account went through"
    );

    // The full set still works, so the failures above are about the missing accounts.
    await swapWith([STUB_ID, listAddress(mint), extra]);
  });

  it("runs the limit order v2 instructions on a hook pool", async () => {
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    await createEmptyList(mint);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [mint, plain] : [plain, mint];
    const [poolState] = await pda.getPoolStatePDA(setup.ammConfig, mint0, mint1);

    // The order pays in whichever side the hook mint is, so the transfer goes through the hook.
    const zeroForOne = hookFirst;
    const hookAta = getAssociatedTokenAddressSync(
      mint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    const plainAta = getAssociatedTokenAddressSync(plain, owner.publicKey);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          hookAta,
          owner.publicKey,
          mint,
          TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(
          mint,
          hookAta,
          owner.publicKey,
          1_000_000_000,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );
    assert.isDefined(plainAta);

    const poolData = await program.account.poolState.fetch(poolState);
    const tickIndex = getValidTickForLimitOrder(
      poolData.tickCurrent,
      poolData.tickSpacing,
      zeroForOne,
      0
    );
    // The hook names the order's tick array, so each instruction must release its borrow on
    // it before the transfer CPI.
    const [tickArray] = await pda.getTickArrayStatePDA(
      poolState,
      getTickArrayStartIndex(tickIndex, poolData.tickSpacing)
    );
    await rewriteList(listAddress(mint), [literalMeta(tickArray)]);
    const bag: AccountMeta[] = [
      { pubkey: STUB_ID, isSigner: false, isWritable: false },
      { pubkey: listAddress(mint), isSigner: false, isWritable: false },
      { pubkey: tickArray, isSigner: false, isWritable: false },
    ];
    // Legs are named after the order's input / output, like the accounts.
    const hookSlice = { accountsType: TRANSFER_HOOK_INPUT, length: bag.length };
    // `openLimitOrder` also passes the bitmap extension, ahead of the bag.
    const v2Open = {
      info: {
        slices: [
          { accountsType: TICK_ARRAY_BITMAP_EXTENSION, length: 1 },
          hookSlice,
        ],
      },
      accounts: bag,
    };
    const v2 = { info: { slices: [hookSlice] }, accounts: bag };

    const opened = await instructions.openLimitOrder({
      owner,
      poolState,
      tickIndex,
      zeroForOne,
      amount: new anchor.BN(1_000_000),
      v2: v2Open,
    });
    const afterOpen = await program.account.limitOrderState.fetch(
      opened.limitOrder
    );
    assert.equal(afterOpen.totalAmount.toNumber(), 1_000_000);

    // increase only reads the input leg: an output segment is ignored and the input transfer
    // goes out without its hook accounts.
    let wrongLegErr: any;
    try {
      await instructions.increaseLimitOrder({
        owner,
        poolState,
        limitOrder: opened.limitOrder,
        amount: new anchor.BN(500_000),
        v2: {
          info: { slices: [{ accountsType: TRANSFER_HOOK_OUTPUT, length: bag.length }] },
          accounts: bag,
        },
      });
    } catch (e) {
      wrongLegErr = e;
    }
    assert.isDefined(wrongLegErr, "a hook transfer went through without its accounts");
    assert.include(String(wrongLegErr), "An account required by the instruction is missing");

    await instructions.increaseLimitOrder({
      owner,
      poolState,
      limitOrder: opened.limitOrder,
      amount: new anchor.BN(500_000),
      v2,
    });
    const afterIncrease = await program.account.limitOrderState.fetch(
      opened.limitOrder
    );
    assert.equal(afterIncrease.totalAmount.toNumber(), 1_500_000);

    await instructions.decreaseLimitOrder({
      owner,
      poolState,
      limitOrder: opened.limitOrder,
      amount: new anchor.BN(500_000),
      amountMin: new anchor.BN(0),
      v2,
    });
    const afterDecrease = await program.account.limitOrderState.fetch(
      opened.limitOrder
    );
    assert.equal(afterDecrease.totalAmount.toNumber(), 1_000_000);

    // `settle_limit_order_v2` is not covered: settling needs the order filled first, and
    // driving a swap through its tick reliably is a piece of scaffolding worth more than the
    // one instruction it would exercise. The v1 path is refused for hook mints either way.
  });

  /// Register a whitelisted hook mint as reward 0 of `poolState` and fund it.
  async function initHookReward(
    poolState: PublicKey,
    opts: { metas?: Buffer[]; bagExtra?: AccountMeta[] } = {}
  ) {
    const rewardMint = await createHookMint();
    const rewardSupport = await whitelist(rewardMint);
    const rewardList = await createEmptyList(rewardMint, opts.metas);

    // `initialize_reward` reads the operation state to decide who may fund. Nothing else in
    // this suite creates it.
    const [operationState] = PublicKey.findProgramAddressSync(
      [Buffer.from("operation")],
      program.programId
    );
    if ((await connection.getAccountInfo(operationState)) === null) {
      await program.methods
        .createOperationAccount()
        .accounts({
          owner: owner.publicKey,
          operationState,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([owner])
        .rpc();
    }

    const funderAta = getAssociatedTokenAddressSync(
      rewardMint,
      owner.publicKey,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey,
          funderAta,
          owner.publicKey,
          rewardMint,
          TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(
          rewardMint,
          funderAta,
          owner.publicKey,
          1_000_000_000_000,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );

    const [rewardVault] = await pda.getPoolRewardVaultPDA(
      poolState,
      rewardMint
    );

    // The admission segment is what `validate_hook_mint` searches, so the list PDA goes there
    // alongside the whitelist entry; the reward leg's own bag is separate.
    const rewardBag: AccountMeta[] = [
      { pubkey: STUB_ID, isSigner: false, isWritable: false },
      { pubkey: rewardList, isSigner: false, isWritable: false },
      ...(opts.bagExtra ?? []),
    ];
    // Off the chain clock, not the host's: surfpool forks mainnet and its slot time is its
    // own. The window also has to sit between MIN_REWARD_PERIOD and MAX_REWARD_PERIOD.
    const chainNow = await chainTime(connection);
    // Without the `paramset` feature the window must be at least 7 days.
    const openTime = new anchor.BN(chainNow + 60);
    const endTime = new anchor.BN(chainNow + 60 + 14 * 24 * 60 * 60);

    await program.methods
      .initializeRewardV2(
        {
          openTime,
          endTime,
          emissionsPerSecondX64: new anchor.BN(1).shln(64),
        },
        {
          slices: [
            { accountsType: MINT_ADMISSION, length: 2 },
            { accountsType: TRANSFER_HOOK_REWARD_0, length: rewardBag.length },
          ],
        }
      )
      .accounts({
        rewardFunder: owner.publicKey,
        funderTokenAccount: funderAta,
        ammConfig: setup.ammConfig,
        poolState,
        operationState,
        rewardTokenMint: rewardMint,
        rewardTokenVault: rewardVault,
        rewardTokenProgram: TOKEN_2022_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .remainingAccounts([
        { pubkey: rewardSupport, isSigner: false, isWritable: false },
        { pubkey: rewardList, isSigner: false, isWritable: false },
        ...rewardBag,
      ])
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([owner])
      .rpc();

    return { rewardMint, rewardVault, rewardBag, funderAta, operationState, openTime, endTime };
  }

  it("initializes and funds a hook reward mint through the v2 instructions", async () => {
    // A pool to hang the reward off; its mints are plain, only the reward carries a hook.
    const poolState = await instructions.createPool({
      poolCreator: owner,
      ammConfig: setup.ammConfig,
      tokenMint0: setup.token0,
      tokenMint1: setup.token1,
    });

    const { rewardMint, rewardVault, rewardBag, funderAta, operationState, openTime, endTime } =
      await initHookReward(poolState);

    const pool = await program.account.poolState.fetch(poolState);
    assert.equal(
      pool.rewardInfos[0].tokenMint.toBase58(),
      rewardMint.toBase58(),
      "initialize_reward_v2 did not register the reward mint"
    );
    const vaultBalance = await connection.getTokenAccountBalance(rewardVault);
    assert.isTrue(
      Number(vaultBalance.value.amount) > 0,
      "the reward vault was not funded through the hook"
    );

    // `set_reward_params_v2`: extending the window tops the vault up, which is another hook
    // transfer. The reward group is a (vault, funder account, mint) triple.
    const rewardGroup: AccountMeta[] = [
      { pubkey: rewardVault, isSigner: false, isWritable: true },
      { pubkey: funderAta, isSigner: false, isWritable: true },
      { pubkey: rewardMint, isSigner: false, isWritable: false },
    ];
    await program.methods
      .setRewardParamsV2(
        0,
        new anchor.BN(2).shln(64),
        openTime,
        endTime.addn(24 * 60 * 60),
        {
          slices: [
            { accountsType: REWARD_GROUP, length: rewardGroup.length },
            { accountsType: TRANSFER_HOOK_REWARD_0, length: rewardBag.length },
          ],
        }
      )
      .accounts({
        authority: owner.publicKey,
        ammConfig: setup.ammConfig,
        poolState,
        operationState,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
      } as any)
      .remainingAccounts([...rewardGroup, ...rewardBag])
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([owner])
      .rpc();

    const afterSet = await connection.getTokenAccountBalance(rewardVault);
    assert.isTrue(
      Number(afterSet.value.amount) > Number(vaultBalance.value.amount),
      "set_reward_params_v2 did not top the vault up through the hook"
    );

  });

  /// A TransferHook extension with no program yet, whether or not one can be set later.
  it("admits dormant and inert hook extensions without the whitelist", async () => {
    async function createUnsetHookMint(authority: PublicKey): Promise<PublicKey> {
      const mintKeypair = Keypair.generate();
      const space = getMintLen([ExtensionType.TransferHook]);
      await sendAndConfirmTransaction(
        connection,
        new Transaction().add(
          SystemProgram.createAccount({
            fromPubkey: owner.publicKey,
            newAccountPubkey: mintKeypair.publicKey,
            space,
            lamports: await connection.getMinimumBalanceForRentExemption(space),
            programId: TOKEN_2022_PROGRAM_ID,
          }),
          createInitializeTransferHookInstruction(
            mintKeypair.publicKey,
            authority,
            PublicKey.default,
            TOKEN_2022_PROGRAM_ID
          ),
          createInitializeMintInstruction(
            mintKeypair.publicKey,
            9,
            owner.publicKey,
            null,
            TOKEN_2022_PROGRAM_ID
          )
        ),
        [owner, mintKeypair]
      );
      return mintKeypair.publicKey;
    }

    const dormant = await createUnsetHookMint(owner.publicKey);
    await createPool(dormant);

    // token-2022 refuses to initialize the extension with neither field set; an inert one is
    // made by dropping the authority afterwards.
    const inert = await createUnsetHookMint(owner.publicKey);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createSetAuthorityInstruction(
          inert,
          owner.publicKey,
          AuthorityType.TransferHookProgramId,
          null,
          [],
          TOKEN_2022_PROGRAM_ID
        )
      ),
      [owner]
    );
    await createPool(inert);
  });

  /// One position on a plain pool whose only hook mint is reward 0.
  async function hookRewardFixture() {
    const [mint0, mint1] = await createMintPair(connection, owner);
    const poolState = await instructions.createPool({
      poolCreator: owner,
      ammConfig: setup.ammConfig,
      tokenMint0: mint0,
      tokenMint1: mint1,
    });
    const reward = await initHookReward(poolState);
    const position = await instructions.openPositionV3({
      payer: owner,
      positionNftOwner: owner.publicKey,
      poolState,
      tickLowerIndex: -120,
      tickUpperIndex: 120,
      liquidity: new anchor.BN(1_000_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      tokenVault0Mint: mint0,
      tokenVault1Mint: mint1,
      token0Program: TOKEN_PROGRAM_ID,
      token1Program: TOKEN_PROGRAM_ID,
      remainingAccountsInfo: { slices: [] },
      remainingAccounts: [],
    });
    const liquidityArgs = {
      owner,
      position,
      mint0,
      mint1,
      token0Program: TOKEN_PROGRAM_ID,
      token1Program: TOKEN_PROGRAM_ID,
    };
    return { poolState, position, liquidityArgs, ...reward };
  }

  async function owedReward0(personalPosition: PublicKey): Promise<anchor.BN> {
    const p = await program.account.personalPositionState.fetch(personalPosition);
    return p.rewardInfos[0].rewardAmountOwed;
  }

  async function balance(account: PublicKey): Promise<bigint> {
    return BigInt((await connection.getTokenAccountBalance(account)).value.amount);
  }

  /// Settle accrued rewards into `reward_amount_owed` without paying anything out.
  async function settleRewards(liquidityArgs: any) {
    await instructions.decreaseLiquidityV3({
      ...liquidityArgs,
      liquidity: new anchor.BN(1_000),
      amount0Min: new anchor.BN(0),
      amount1Min: new anchor.BN(0),
      remainingAccountsInfo: { slices: [] },
      remainingAccounts: [],
    });
  }

  it("pays, forfeits and reclaims a hook reward through the v3 reward instructions", async () => {
    const f = await hookRewardFixture();
    await timeTravelTo(connection, f.openTime.toNumber() + 24 * 60 * 60);

    // Legacy decrease has no room for the hook's accounts: the principal comes out and the
    // hook reward is left owed rather than failing the withdrawal.
    const legacyRecipient0 = getAssociatedTokenAddressSync(f.liquidityArgs.mint0, owner.publicKey);
    const principalBefore = await balance(legacyRecipient0);
    const rewardBefore = await balance(f.funderAta);
    await program.methods
      .decreaseLiquidityV2(new anchor.BN(1_000), new anchor.BN(0), new anchor.BN(0))
      .accounts({
        nftOwner: owner.publicKey,
        nftAccount: f.position.positionNftAccount,
        personalPosition: f.position.personalPosition,
        poolState: f.poolState,
        protocolPosition: f.position.protocolPosition,
        tokenVault0: f.position.tokenVault0,
        tokenVault1: f.position.tokenVault1,
        tickArrayLower: f.position.tickArrayLower,
        tickArrayUpper: f.position.tickArrayUpper,
        recipientTokenAccount0: getAssociatedTokenAddressSync(f.liquidityArgs.mint0, owner.publicKey),
        recipientTokenAccount1: getAssociatedTokenAddressSync(f.liquidityArgs.mint1, owner.publicKey),
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        memoProgram: MEMO_PROGRAM_ID,
        vault0Mint: f.liquidityArgs.mint0,
        vault1Mint: f.liquidityArgs.mint1,
      } as any)
      .remainingAccounts([
        { pubkey: f.rewardVault, isSigner: false, isWritable: true },
        { pubkey: f.funderAta, isSigner: false, isWritable: true },
        { pubkey: f.rewardMint, isSigner: false, isWritable: false },
      ])
      .preInstructions([ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 })])
      .signers([owner])
      .rpc();
    assert.isTrue((await balance(legacyRecipient0)) > principalBefore, "no principal came out");
    assert.equal(await balance(f.funderAta), rewardBefore, "a hook reward was paid without its accounts");

    const owed = await owedReward0(f.position.personalPosition);
    assert.isTrue(owed.gtn(0), "the skipped hook reward was not kept owed");

    const collectAccounts = {
      nftOwner: owner.publicKey,
      nftAccount: f.position.positionNftAccount,
      personalPosition: f.position.personalPosition,
      poolState: f.poolState,
      tickArrayLower: f.position.tickArrayLower,
      tickArrayUpper: f.position.tickArrayUpper,
      rewardTokenVault: f.rewardVault,
      recipientTokenAccount: f.funderAta,
      rewardVaultMint: f.rewardMint,
      tokenProgram: TOKEN_PROGRAM_ID,
      tokenProgram2022: TOKEN_2022_PROGRAM_ID,
    } as any;
    const rewardTape = {
      slices: [{ accountsType: TRANSFER_HOOK_REWARD_0, length: f.rewardBag.length }],
    };

    // Without the hook's accounts the transfer cannot be built.
    let noBagErr: any;
    try {
      await program.methods
        .collectReward(0, { slices: [] })
        .accounts(collectAccounts)
        .signers([owner])
        .rpc();
    } catch (e) {
      noBagErr = e;
    }
    assert.isDefined(noBagErr, "collect_reward moved a hook reward without its accounts");

    const claimedBefore = (await program.account.poolState.fetch(f.poolState)).rewardInfos[0]
      .rewardClaimed;
    const recipientBefore = await balance(f.funderAta);
    const collect = () =>
      program.methods
        .collectReward(0, rewardTape)
        .accounts(collectAccounts)
        .remainingAccounts(f.rewardBag)
        .signers([owner])
        .rpc();
    await collect();
    // Settled up to now first: at least what was owed, plus what accrued since.
    const paid = (await balance(f.funderAta)) - recipientBefore;
    assert.isTrue(paid >= BigInt(owed.toString()), `paid ${paid} < owed ${owed}`);
    assert.isTrue((await owedReward0(f.position.personalPosition)).isZero());
    let claimedAfterCollect = (await program.account.poolState.fetch(f.poolState))
      .rewardInfos[0].rewardClaimed;
    assert.equal(BigInt(claimedAfterCollect.sub(claimedBefore).toString()), paid);

    // Nothing settled in between: collect alone pays what accrued since the last collect.
    await timeTravelTo(connection, (await chainTime(connection)) + 24 * 60 * 60);
    const beforeDirect = await balance(f.funderAta);
    await collect();
    const paidDirect = (await balance(f.funderAta)) - beforeDirect;
    assert.isTrue(paidDirect > 0n, "collect_reward paid nothing for a day of accrual");
    const claimedAfterDirect = (await program.account.poolState.fetch(f.poolState))
      .rewardInfos[0].rewardClaimed;
    assert.equal(BigInt(claimedAfterDirect.sub(claimedAfterCollect).toString()), paidDirect);
    claimedAfterCollect = claimedAfterDirect;

    // Forfeit: the owed amount is written off as claimed, nothing moves.
    await timeTravelTo(connection, (await chainTime(connection)) + 24 * 60 * 60);
    await settleRewards(f.liquidityArgs);
    const owed2 = await owedReward0(f.position.personalPosition);
    assert.isTrue(owed2.gtn(0), "no reward accrued for the forfeit");
    const forfeitAccounts = {
      nftOwner: owner.publicKey,
      nftAccount: f.position.positionNftAccount,
      personalPosition: f.position.personalPosition,
      poolState: f.poolState,
    } as any;
    const recipientBeforeForfeit = await balance(f.funderAta);
    await program.methods.forfeitReward(0).accounts(forfeitAccounts).signers([owner]).rpc();
    assert.isTrue((await owedReward0(f.position.personalPosition)).isZero());
    const claimedAfterForfeit = (await program.account.poolState.fetch(f.poolState))
      .rewardInfos[0].rewardClaimed;
    assert.isTrue(claimedAfterForfeit.sub(claimedAfterCollect).eq(owed2));
    assert.equal(await balance(f.funderAta), recipientBeforeForfeit);

    // Nothing owed: a no-op rather than an error.
    await program.methods.forfeitReward(0).accounts(forfeitAccounts).signers([owner]).rpc();
    const claimedAfterNoop = (await program.account.poolState.fetch(f.poolState))
      .rewardInfos[0].rewardClaimed;
    assert.isTrue(claimedAfterNoop.eq(claimedAfterForfeit));

    // After the window closes the funder takes back everything not still owed to positions.
    await timeTravelTo(connection, f.endTime.toNumber() + 60);
    const vaultBefore = await balance(f.rewardVault);
    const funderBefore = await balance(f.funderAta);
    await program.methods
      .collectRemainingRewardsV2(0, rewardTape)
      .accounts({
        rewardFunder: owner.publicKey,
        funderTokenAccount: f.funderAta,
        poolState: f.poolState,
        rewardTokenVault: f.rewardVault,
        rewardVaultMint: f.rewardMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        memoProgram: MEMO_PROGRAM_ID,
      } as any)
      .remainingAccounts(f.rewardBag)
      .signers([owner])
      .rpc();
    const info = (await program.account.poolState.fetch(f.poolState)).rewardInfos[0];
    const unclaimed = BigInt(info.rewardTotalEmitted.sub(info.rewardClaimed).toString());
    const vaultAfter = await balance(f.rewardVault);
    assert.equal(vaultAfter, unclaimed, "the vault must keep exactly what positions are still owed");
    assert.equal(await balance(f.funderAta) - funderBefore, vaultBefore - vaultAfter);
  });

  /// Deposits run with `pool_state` borrowed by the program; a hook that pulls the pool PDA
  /// into the CPI needs that borrow released first.
  it("deposits through a hook whose extras resolve to the pool PDA", async () => {
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    const list = await createEmptyList(mint, [DESTINATION_OWNER_META]);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [mint, plain] : [plain, mint];
    const [program0, program1] = hookFirst
      ? [TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID]
      : [TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID];
    const [poolState] = await pda.getPoolStatePDA(setup.ammConfig, mint0, mint1);
    const hookAta = getAssociatedTokenAddressSync(mint, owner.publicKey, false, TOKEN_2022_PROGRAM_ID);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey, hookAta, owner.publicKey, mint, TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(mint, hookAta, owner.publicKey, 1_000_000_000, [], TOKEN_2022_PROGRAM_ID)
      ),
      [owner]
    );

    const bag: AccountMeta[] = [
      { pubkey: STUB_ID, isSigner: false, isWritable: false },
      { pubkey: list, isSigner: false, isWritable: false },
      { pubkey: poolState, isSigner: false, isWritable: false },
    ];
    const tape = {
      slices: [{ accountsType: hookFirst ? TRANSFER_HOOK_TOKEN_0 : TRANSFER_HOOK_TOKEN_1, length: bag.length }],
    };
    const position = await instructions.openPositionV3({
      payer: owner,
      positionNftOwner: owner.publicKey,
      poolState,
      tickLowerIndex: -120,
      tickUpperIndex: 120,
      liquidity: new anchor.BN(1_000_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      tokenVault0Mint: mint0,
      tokenVault1Mint: mint1,
      token0Program: program0,
      token1Program: program1,
      remainingAccountsInfo: tape,
      remainingAccounts: bag,
    });
    await instructions.increaseLiquidityV3({
      owner,
      position,
      mint0,
      mint1,
      token0Program: program0,
      token1Program: program1,
      liquidity: new anchor.BN(500_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      remainingAccountsInfo: tape,
      remainingAccounts: bag,
    });
    const after = await program.account.personalPositionState.fetch(position.personalPosition);
    assert.isTrue(after.liquidity.eq(new anchor.BN(1_500_000)));

    // Reward funding and top-up deposit into the reward vault, also owned by the pool PDA.
    const [p0, p1] = await createMintPair(connection, owner);
    const rewardPool = await instructions.createPool({
      poolCreator: owner,
      ammConfig: setup.ammConfig,
      tokenMint0: p0,
      tokenMint1: p1,
    });
    const r = await initHookReward(rewardPool, {
      metas: [DESTINATION_OWNER_META],
      bagExtra: [{ pubkey: rewardPool, isSigner: false, isWritable: false }],
    });
    const funded = await balance(r.rewardVault);
    assert.isTrue(funded > 0n, "initialize_reward_v2 did not fund the vault");

    const rewardGroup: AccountMeta[] = [
      { pubkey: r.rewardVault, isSigner: false, isWritable: true },
      { pubkey: r.funderAta, isSigner: false, isWritable: true },
      { pubkey: r.rewardMint, isSigner: false, isWritable: false },
    ];
    await program.methods
      .setRewardParamsV2(0, new anchor.BN(2).shln(64), r.openTime, r.endTime.addn(24 * 60 * 60), {
        slices: [
          { accountsType: REWARD_GROUP, length: rewardGroup.length },
          { accountsType: TRANSFER_HOOK_REWARD_0, length: r.rewardBag.length },
        ],
      })
      .accounts({
        authority: owner.publicKey,
        ammConfig: setup.ammConfig,
        poolState: rewardPool,
        operationState: r.operationState,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
      } as any)
      .remainingAccounts([...rewardGroup, ...r.rewardBag])
      .preInstructions([ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 })])
      .signers([owner])
      .rpc();
    assert.isTrue((await balance(r.rewardVault)) > funded, "set_reward_params_v2 did not top up");
  });

  it("withdraws after the hook grows its list past the admission cap", async () => {
    const mint = await createHookMint();
    const supportMint = await whitelist(mint);
    const list = await createEmptyList(mint);
    await createPool(mint, { whitelist: supportMint, list: true });

    const plain = setup.token0;
    const hookFirst = mint.toBuffer().compare(plain.toBuffer()) < 0;
    const [mint0, mint1] = hookFirst ? [mint, plain] : [plain, mint];
    const [program0, program1] = hookFirst
      ? [TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID]
      : [TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID];
    const [poolState] = await pda.getPoolStatePDA(setup.ammConfig, mint0, mint1);
    const hookAta = getAssociatedTokenAddressSync(mint, owner.publicKey, false, TOKEN_2022_PROGRAM_ID);
    await sendAndConfirmTransaction(
      connection,
      new Transaction().add(
        createAssociatedTokenAccountIdempotentInstruction(
          owner.publicKey, hookAta, owner.publicKey, mint, TOKEN_2022_PROGRAM_ID
        ),
        createMintToInstruction(mint, hookAta, owner.publicKey, 1_000_000_000, [], TOKEN_2022_PROGRAM_ID)
      ),
      [owner]
    );
    const hookType = hookFirst ? TRANSFER_HOOK_TOKEN_0 : TRANSFER_HOOK_TOKEN_1;
    const liquidityArgs = {
      mint0,
      mint1,
      token0Program: program0,
      token1Program: program1,
    };
    const position = await instructions.openPositionV3({
      payer: owner,
      positionNftOwner: owner.publicKey,
      poolState,
      tickLowerIndex: -120,
      tickUpperIndex: 120,
      liquidity: new anchor.BN(1_000_000),
      amount0Max: new anchor.BN(1_000_000_000),
      amount1Max: new anchor.BN(1_000_000_000),
      tokenVault0Mint: mint0,
      tokenVault1Mint: mint1,
      ...liquidityArgs,
      remainingAccountsInfo: { slices: [{ accountsType: hookType, length: 2 }] },
      remainingAccounts: [
        { pubkey: STUB_ID, isSigner: false, isWritable: false },
        { pubkey: list, isSigner: false, isWritable: false },
      ],
    });

    // The hook grows its list after the pool exists, past the 6 extras checked at creation.
    const extras = Array.from({ length: 9 }, () => Keypair.generate().publicKey);
    await rewriteList(list, extras.map(literalMeta));

    const bag: AccountMeta[] = [
      { pubkey: STUB_ID, isSigner: false, isWritable: false },
      { pubkey: list, isSigner: false, isWritable: false },
      ...extras.map((pubkey) => ({ pubkey, isSigner: false, isWritable: false })),
    ];
    const hookBefore = await balance(hookAta);
    await instructions.decreaseLiquidityV3({
      owner,
      position,
      ...liquidityArgs,
      liquidity: new anchor.BN(1_000_000),
      amount0Min: new anchor.BN(0),
      amount1Min: new anchor.BN(0),
      remainingAccountsInfo: { slices: [{ accountsType: hookType, length: bag.length }] },
      remainingAccounts: bag,
    });
    assert.isTrue((await balance(hookAta)) > hookBefore, "the LP got nothing back");
  });

});
