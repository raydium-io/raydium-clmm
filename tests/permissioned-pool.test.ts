import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import {
  PublicKey,
  Keypair,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  ComputeBudgetProgram,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import { PDAUtils } from "./utils/pda";
import { getTickArrayAddressByTick } from "./utils/util";
import {
  TickUtil,
  TickArrayUtil,
  MEMO_PROGRAM_ID,
  METADATA_PROGRAM_ID,
} from "@raydium-io/raydium-sdk-v2";

describe("create_permissioned_pool", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const setup = new TestSetup(program, user);
  const instructions = new InstructionHelper(program);
  const pda = new PDAUtils(program.programId);

  // `create_permission_pda` / `close_permission_pda` gate their `owner` signer
  // on `crate::admin::ID`. `yarn test:local-admin` builds with
  // `--features localnet` and compiles the local wallet in as that admin, so
  // the cases below can sign as admin; otherwise they are skipped.
  const admin = provider.wallet.payer;
  const adminIsLocalWallet =
    process.env.CLMM_LOCALNET_ADMIN === admin.publicKey.toBase58();
  const SKIP_MSG =
    "needs a `--features localnet` build with CLMM_LOCALNET_ADMIN=<local wallet> (yarn test:local-admin)";

  before(async () => {
    await setup.initialize();
    await setup.createTokens();
    await setup.mintTokens();
    await setup.createAmmConfig(0);
  });

  /**
   * Calls `create_permissioned_pool` for the given payer/poolCreator/seedIndex
   * against the shared (ammConfig, token0, token1) pair set up in `before()`,
   * and returns the derived pool_state PDA.
   *
   * `payer` pays rent and must hold the permission PDA (the gate checks
   * `[PERMISSION_SEED, payer]`); it is the only signer besides the ambient
   * test wallet paying for the transaction fee. `poolCreator` is an arbitrary
   * (non-signing) address that becomes the pool's `owner`.
   */
  async function callCreatePermissionedPool(params: {
    payer: Keypair;
    poolCreator: PublicKey;
    seedIndex: number;
  }): Promise<PublicKey> {
    const [poolState] = await pda.getPermissionedPoolPDA(
      setup.ammConfig,
      setup.token0,
      setup.token1,
      params.seedIndex,
    );
    const [permission] = await pda.getPermissionPDA(params.payer.publicKey);
    const [tokenVault0] = await pda.getTokenVaultPDA(poolState, setup.token0);
    const [tokenVault1] = await pda.getTokenVaultPDA(poolState, setup.token1);
    const [observationState] = await pda.getObservationStatePDA(poolState);
    const [tickArrayBitmap] = await pda.getTickArrayBitmapPDA(poolState);

    await program.methods
      .createPermissionedPool(
        {
          sqrtPriceX64: TickUtil.getSqrtPriceAtTick(0),
          collectFeeOn: { fromInput: {} } as any,
          enableDynamicFee: false,
        },
        params.seedIndex,
      )
      .accounts({
        payer: params.payer.publicKey,
        poolCreator: params.poolCreator,
        permission,
        ammConfig: setup.ammConfig,
        poolState,
        tokenMint0: setup.token0,
        tokenMint1: setup.token1,
        tokenVault0,
        tokenVault1,
        observationState,
        tickArrayBitmap,
        tokenProgram0: TOKEN_PROGRAM_ID,
        tokenProgram1: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .signers([params.payer])
      .rpc();

    return poolState;
  }

  describe("with a permission PDA granted by the admin", () => {
    let payer: Keypair;
    let poolCreator: PublicKey;

    before(async function () {
      if (!adminIsLocalWallet) {
        console.log(`      [skipped] ${SKIP_MSG}`);
        this.skip();
      }

      payer = Keypair.generate();
      await instructions.airdrop(payer.publicKey, 5);
      // Arbitrary, non-signing address that becomes the pool's `owner`.
      poolCreator = Keypair.generate().publicKey;
      const [permission] = await pda.getPermissionPDA(payer.publicKey);

      await program.methods
        .createPermissionPda()
        .accounts({
          owner: admin.publicKey,
          permissionAuthority: payer.publicKey,
          permission,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([admin])
        .rpc();
    });

    it("authorized payer opens two pools for one pair with different seed_index", async () => {
      const poolOne = await callCreatePermissionedPool({
        payer,
        poolCreator,
        seedIndex: 1,
      });
      const poolTwo = await callCreatePermissionedPool({
        payer,
        poolCreator,
        seedIndex: 2,
      });

      assert.isFalse(
        poolOne.equals(poolTwo),
        "pool PDAs for different seed_index values must differ",
      );

      const poolOneData = await program.account.poolState.fetch(poolOne);
      const poolTwoData = await program.account.poolState.fetch(poolTwo);

      assert.equal(
        Buffer.from(poolOneData.seedIndex).readUInt16LE(0),
        1,
        "pool created with seedIndex=1 must store seed_index=1",
      );
      assert.equal(
        Buffer.from(poolTwoData.seedIndex).readUInt16LE(0),
        2,
        "pool created with seedIndex=2 must store seed_index=2",
      );
    });

    it("rejects seedIndex == 0", async () => {
      try {
        await callCreatePermissionedPool({ payer, poolCreator, seedIndex: 0 });
        assert.fail("Should have thrown RequireGtViolated for seedIndex == 0");
      } catch (e: any) {
        assert.include(
          e.toString(),
          "RequireGtViolated",
          `expected require_gt! violation, got: ${e.toString()}`,
        );
      }
    });
  });

  it("rejects a payer without a permission PDA", async () => {
    const stranger = Keypair.generate();
    await instructions.airdrop(stranger.publicKey, 5);
    const poolCreator = Keypair.generate().publicKey;

    try {
      await callCreatePermissionedPool({ payer: stranger, poolCreator, seedIndex: 7 });
      assert.fail("Should have thrown - stranger holds no permission PDA");
    } catch (e: any) {
      assert.match(
        e.toString(),
        /AccountNotInitialized/,
        `expected AccountNotInitialized (payer holds no permission PDA), got: ${e.toString()}`,
      );
    }
  });

  it("closes a permission PDA (admin), refunding rent and removing the account", async function () {
    if (!adminIsLocalWallet) {
      console.log(`      [skipped] ${SKIP_MSG}`);
      this.skip();
    }

    const authority = Keypair.generate().publicKey;
    const [permission] = await pda.getPermissionPDA(authority);

    await program.methods
      .createPermissionPda()
      .accounts({
        owner: admin.publicKey,
        permissionAuthority: authority,
        permission,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([admin])
      .rpc();

    assert.isNotNull(
      await provider.connection.getAccountInfo(permission),
      "permission PDA should exist after create_permission_pda",
    );

    // No `systemProgram` account here: it was intentionally removed from the
    // close instruction (a plain `close =` needs none). This exercises that.
    await program.methods
      .closePermissionPda()
      .accounts({
        owner: admin.publicKey,
        permissionAuthority: authority,
        permission,
      } as any)
      .signers([admin])
      .rpc();

    assert.isNull(
      await provider.connection.getAccountInfo(permission),
      "permission PDA should be closed (account removed) after close_permission_pda",
    );
  });

  it("legacy create_pool pool still swaps and withdraws after the layout change", async () => {
    const poolState = await setup.createPool(0);
    const poolStateData = await program.account.poolState.fetch(poolState);

    assert.deepEqual(
      poolStateData.seedIndex,
      [0, 0],
      "legacy (non-seeded) pool must report seed_index == [0, 0]",
    );

    // --- Open a position so the pool has liquidity to trade against ---
    const tickLowerIndex = -60;
    const tickUpperIndex = 60;
    const tickSpacing = poolStateData.tickSpacing;

    const lowerStart = TickArrayUtil.getTickArrayStartIndex(tickLowerIndex, tickSpacing);
    const upperStart = TickArrayUtil.getTickArrayStartIndex(tickUpperIndex, tickSpacing);

    const tickArrayLower = getTickArrayAddressByTick(
      program.programId,
      poolState,
      lowerStart,
      tickSpacing,
    );
    const tickArrayUpper = getTickArrayAddressByTick(
      program.programId,
      poolState,
      upperStart,
      tickSpacing,
    );

    const positionNftMint = Keypair.generate();
    const positionNftAccount = getAssociatedTokenAddressSync(
      positionNftMint.publicKey,
      user.publicKey,
    );
    const [personalPosition] = await pda.getPersonalPositionStatePDA(
      positionNftMint.publicKey,
    );
    const [protocolPosition] = await pda.getProtocolPositionStatePDA(
      poolState,
      tickLowerIndex,
      tickUpperIndex,
    );
    const [metadataAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        METADATA_PROGRAM_ID.toBuffer(),
        positionNftMint.publicKey.toBuffer(),
      ],
      METADATA_PROGRAM_ID,
    );
    const tokenAccount0 = getAssociatedTokenAddressSync(setup.token0, user.publicKey);
    const tokenAccount1 = getAssociatedTokenAddressSync(setup.token1, user.publicKey);
    const [tokenVault0] = await pda.getTokenVaultPDA(poolState, setup.token0);
    const [tokenVault1] = await pda.getTokenVaultPDA(poolState, setup.token1);

    await program.methods
      .openPosition(
        tickLowerIndex,
        tickUpperIndex,
        lowerStart,
        upperStart,
        new anchor.BN(1_000_000_000),
        new anchor.BN(10_000_000_000),
        new anchor.BN(10_000_000_000),
      )
      .accounts({
        payer: user.publicKey,
        positionNftOwner: user.publicKey,
        positionNftMint: positionNftMint.publicKey,
        positionNftAccount: positionNftAccount,
        metadataAccount: metadataAccount,
        poolState: poolState,
        protocolPosition: protocolPosition,
        tickArrayLower: tickArrayLower,
        tickArrayUpper: tickArrayUpper,
        personalPosition: personalPosition,
        tokenAccount0: tokenAccount0,
        tokenAccount1: tokenAccount1,
        tokenVault0: tokenVault0,
        tokenVault1: tokenVault1,
        rent: SYSVAR_RENT_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
      } as any)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }),
      ])
      .signers([user, positionNftMint])
      .rpc();

    // --- Swap: pool must still trade correctly with the new layout ---
    // tick_spacing is 10 for ammConfig index 0, so ticks_in_array = 600 and
    // tick 0 sits exactly on the boundary between the tick array covering
    // [-600, 0) and the one covering [0, 600). The position's lower/upper
    // ticks (-60/60) each anchor one of those two arrays, so a zero_for_one
    // swap starting at tick 0 and moving down needs both — matching what
    // open_position already derived (tickArrayLower/tickArrayUpper).
    const poolBeforeSwap = await program.account.poolState.fetch(poolState);
    await instructions.swapV2({
      owner: user,
      ammConfig: poolStateData.ammConfig,
      poolState: poolState,
      inputVaultMint: setup.token0,
      outputVaultMint: setup.token1,
      amount: new anchor.BN(1_000_000),
      otherAmountThreshold: new anchor.BN(0),
      sqrtPriceLimitX64: TickUtil.getSqrtPriceAtTick(-30),
      isBaseInput: true,
      remainingAccounts: [
        { pubkey: tickArrayUpper, isWritable: true, isSigner: false },
        { pubkey: tickArrayLower, isWritable: true, isSigner: false },
      ],
    });

    const poolAfterSwap = await program.account.poolState.fetch(poolState);
    assert.notEqual(
      poolAfterSwap.sqrtPriceX64.toString(),
      poolBeforeSwap.sqrtPriceX64.toString(),
      "swap on the legacy pool must move the price",
    );

    // --- Withdraw: full decrease_liquidity_v2 must still succeed ---
    const positionBeforeDecrease = await program.account.personalPositionState.fetch(
      personalPosition,
    );

    await program.methods
      .decreaseLiquidityV2(positionBeforeDecrease.liquidity, new anchor.BN(0), new anchor.BN(0))
      .accounts({
        nftOwner: user.publicKey,
        nftAccount: positionNftAccount,
        personalPosition: personalPosition,
        poolState: poolState,
        protocolPosition: protocolPosition,
        tokenVault0: tokenVault0,
        tokenVault1: tokenVault1,
        tickArrayLower: tickArrayLower,
        tickArrayUpper: tickArrayUpper,
        recipientTokenAccount0: tokenAccount0,
        recipientTokenAccount1: tokenAccount1,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        memoProgram: MEMO_PROGRAM_ID,
        vault0Mint: setup.token0,
        vault1Mint: setup.token1,
      } as any)
      .signers([user])
      .rpc();

    const positionAfterDecrease = await program.account.personalPositionState.fetch(
      personalPosition,
    );
    assert.equal(
      positionAfterDecrease.liquidity.toString(),
      "0",
      "full withdraw must zero out the position's liquidity",
    );
  });
});
