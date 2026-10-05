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
  MEMO_PROGRAM_ID,
  METADATA_PROGRAM_ID,
  getSqrtPriceX64FromTick,
  getTickArrayStartIndex,
} from "./utils/tick-math";

describe("create_customizable_pool_v2", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const setup = new TestSetup(program, user);
  const instructions = new InstructionHelper(program);
  const pda = new PDAUtils(program.programId);

  before(async () => {
    await setup.initialize();
    await setup.createTokens();
    await setup.mintTokens();
    await setup.createAmmConfig(0);
  });

  /**
   * Calls `create_customizable_pool_v2` against the shared (ammConfig, token0,
   * token1) pair and returns the derived pool_state PDA. `poolCreator` is a
   * non-signing address that becomes the pool's `owner`.
   */
  async function callCreateCustomizablePoolV2(params: {
    payer: Keypair;
    poolCreator: PublicKey;
    seedIndex: number;
  }): Promise<PublicKey> {
    const [poolState] = await pda.getCustomizablePoolV2PDA(
      setup.ammConfig,
      setup.token0,
      setup.token1,
      params.seedIndex,
    );
    const [tokenVault0] = await pda.getTokenVaultPDA(poolState, setup.token0);
    const [tokenVault1] = await pda.getTokenVaultPDA(poolState, setup.token1);
    const [observationState] = await pda.getObservationStatePDA(poolState);
    const [tickArrayBitmap] = await pda.getTickArrayBitmapPDA(poolState);

    await program.methods
      .createCustomizablePoolV2(
        {
          sqrtPriceX64: getSqrtPriceX64FromTick(0),
          collectFeeOn: { fromInput: {} } as any,
          enableDynamicFee: false,
        },
        params.seedIndex,
      )
      .accounts({
        payer: params.payer.publicKey,
        poolCreator: params.poolCreator,
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

  describe("with any payer", () => {
    let payer: Keypair;
    let poolCreator: PublicKey;

    before(async () => {
      payer = Keypair.generate();
      await instructions.airdrop(payer.publicKey, 5);
      poolCreator = Keypair.generate().publicKey;
    });

    it("opens two pools for one pair with different seed_index", async () => {
      const poolOne = await callCreateCustomizablePoolV2({
        payer,
        poolCreator,
        seedIndex: 1,
      });
      const poolTwo = await callCreateCustomizablePoolV2({
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
        await callCreateCustomizablePoolV2({ payer, poolCreator, seedIndex: 0 });
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

    const lowerStart = getTickArrayStartIndex(tickLowerIndex, tickSpacing);
    const upperStart = getTickArrayStartIndex(tickUpperIndex, tickSpacing);

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
      sqrtPriceLimitX64: getSqrtPriceX64FromTick(-30),
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
