import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { AccountMeta } from "@solana/web3.js";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import { PDAUtils } from "./utils/pda";
import { SqrtPriceMath, TickUtils } from "@raydium-io/raydium-sdk-v2";
import { getTickArrayBitmapBit, getTickStateByTick, cleanupAllLimitOrders } from "./utils/util";
import { getAccount, getAssociatedTokenAddressSync } from "@solana/spl-token";

describe("swap_test", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const setup = new TestSetup(program, user);
  const instructions = new InstructionHelper(program);
  const pda = new PDAUtils(program.programId);
  let poolState: anchor.web3.PublicKey;

  before(async () => {
    await setup.initialize();
    // Clean up any existing limit orders before running tests
    await cleanupAllLimitOrders(program, instructions, user);
  });

  describe("swapV2", () => {
    it("limit order: zero_for_one = true, price at limitTick — partial fill stays at limitTick-1; full fill moves to limitTick and settles", async () => {
      const poolStateLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(
        poolStateLocal
      );
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;

      await instructions.openPosition({
        payer: user,
        poolState: poolStateLocal,
        tickLowerIndex: -10,
        tickUpperIndex: 10,
        liquidity: new anchor.BN(1_000_000_000),
        amount0Max: new anchor.BN(10_000_000_000),
        amount1Max: new anchor.BN(10_000_000_000),
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateData.tokenMint0,
        tokenVault1Mint: poolStateData.tokenMint1,
      });

      await instructions.openPosition({
        payer: user,
        poolState: poolStateLocal,
        tickLowerIndex: -20,
        tickUpperIndex: 20,
        liquidity: new anchor.BN(1_000_000_000),
        amount0Max: new anchor.BN(10_000_000_000),
        amount1Max: new anchor.BN(10_000_000_000),
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateData.tokenMint0,
        tokenVault1Mint: poolStateData.tokenMint1,
      });

      const ORDER_AMOUNT = new anchor.BN(700_000);
      const SWAP_AMOUNT = new anchor.BN(200_000);
      const limitTick = 10;
      const limitZeroForOne = true;
      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: limitTick,
        zeroForOne: limitZeroForOne,
        amount: ORDER_AMOUNT,
      });

      const inputVaultMint = limitZeroForOne
        ? poolStateData.tokenMint1
        : poolStateData.tokenMint0;
      const outputVaultMint = limitZeroForOne
        ? poolStateData.tokenMint0
        : poolStateData.tokenMint1;

      // Price lands exactly at tick=10; the limit order is not filled
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        amount: new anchor.BN(1000302),
        otherAmountThreshold: new anchor.BN(0),
        sqrtPriceLimitX64: SqrtPriceMath.getSqrtPriceX64FromTick(10),
        isBaseInput: true,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              limitTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateBeforePartial = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );
      assert.isTrue(
        tickStateBeforePartial.ordersAmount.toString() ===
          ORDER_AMOUNT.toString(),
        "tickState.ordersAmount should be equal to ORDER_AMOUNT"
      );
      assert.isTrue(
        tickStateBeforePartial.partFilledOrdersRemaining.toString() === "0",
        "tickState.partFilledOrdersRemaining should be equal to 0"
      );
      assert.isTrue(
        tickStateBeforePartial.unfilledRatioX64.toString() === "0",
        "tickState.unfilledRatioX64 should be 0 before any partial fill"
      );

      // Check pool price and tickCurrent
      const poolBeforePartial = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitBeforePartial =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolBeforePartial.tickCurrent,
        limitTick - 1,
        "tickCurrent should be limitTick when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolBeforePartial.sqrtPriceX64.toString(),
        expectedSqrtAtLimitBeforePartial.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      // Limit order is partially filled
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        amount: SWAP_AMOUNT,
        otherAmountThreshold: new anchor.BN(10_000_000_000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              limitTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateAfterPartial = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );

      assert.isTrue(
        tickStateAfterPartial.ordersAmount.toString() === "0",
        "tickState.ordersAmount should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterPartial.partFilledOrdersRemaining.toString() ===
          ORDER_AMOUNT.sub(SWAP_AMOUNT).toString(),
        "tickState.partFilledOrdersRemaining should be equal to ORDER_AMOUNT - SWAP_AMOUNT"
      );
      assert.isTrue(
        !tickStateAfterPartial.unfilledRatioX64.isZero(),
        "tickState.unfilledRatioX64 should be non-zero after partial fill"
      );

      // Check pool price and tickCurrent
      const poolAfterPartial = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitAfterPartial =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolAfterPartial.tickCurrent,
        limitTick - 1,
        "tickCurrent should be limitTick when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolAfterPartial.sqrtPriceX64.toString(),
        expectedSqrtAtLimitAfterPartial.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      // Limit order is fully filled
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        amount: ORDER_AMOUNT.sub(SWAP_AMOUNT),
        otherAmountThreshold: new anchor.BN(10_000_000_000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              limitTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateAfterFull = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );

      assert.isTrue(
        tickStateAfterFull.ordersAmount.toString() === "0",
        "tickState.ordersAmount should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterFull.partFilledOrdersRemaining.toString() === "0",
        "tickState.partFilledOrdersRemaining should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterFull.unfilledRatioX64.isZero(),
        "tickState.unfilledRatioX64 should collapse to 0 after full fill"
      );

      // Check pool price and tickCurrent
      const poolAfterFull = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitAfterFull =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolAfterFull.tickCurrent,
        limitTick,
        "tickCurrent should be limitTick when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolAfterFull.sqrtPriceX64.toString(),
        expectedSqrtAtLimitAfterFull.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      const poolNewLiquidity = poolAfterPartial.liquidity.add(
        tickStateAfterFull.liquidityNet
      );
      assert.equal(
        poolNewLiquidity.toString(),
        poolAfterFull.liquidity.toString(),
        "poolNewLiquidity should equal poolAfterFull.liquidity"
      );

      // Settle the limit order and verify filledAmount
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const orderState = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(
        orderState.filledAmount.toString(),
        ORDER_AMOUNT.toString(),
        "limit order filledAmount should equal ORDER_AMOUNT after full fill and settlement"
      );
    });

    it("limit order: zero_for_one = false, price at limitTick — partial fill stays at limitTick-1; full fill moves to limitTick and settles", async () => {
      const poolStateLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(
        poolStateLocal
      );
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;

      await instructions.openPosition({
        payer: user,
        poolState: poolStateLocal,
        tickLowerIndex: -10,
        tickUpperIndex: 10,
        liquidity: new anchor.BN(1_000_000_000),
        amount0Max: new anchor.BN(10_000_000_000),
        amount1Max: new anchor.BN(10_000_000_000),
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateData.tokenMint0,
        tokenVault1Mint: poolStateData.tokenMint1,
      });

      await instructions.openPosition({
        payer: user,
        poolState: poolStateLocal,
        tickLowerIndex: -20,
        tickUpperIndex: 20,
        liquidity: new anchor.BN(10_000_000_000),
        amount0Max: new anchor.BN(10_000_000_000),
        amount1Max: new anchor.BN(10_000_000_000),
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateData.tokenMint0,
        tokenVault1Mint: poolStateData.tokenMint1,
      });

      const ORDER_AMOUNT = new anchor.BN(700_000);
      const SWAP_AMOUNT = new anchor.BN(200_000);
      const limitTick = -10;
      const limitZeroForOne = false;
      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: limitTick,
        zeroForOne: limitZeroForOne,
        amount: ORDER_AMOUNT,
      });

      const inputVaultMint = limitZeroForOne
        ? poolStateData.tokenMint1
        : poolStateData.tokenMint0;
      const outputVaultMint = limitZeroForOne
        ? poolStateData.tokenMint0
        : poolStateData.tokenMint1;

      // Price lands exactly at tick=-10; the limit order is not filled
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        amount: new anchor.BN(5501652),
        otherAmountThreshold: new anchor.BN(0),
        sqrtPriceLimitX64: SqrtPriceMath.getSqrtPriceX64FromTick(limitTick),
        isBaseInput: true,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              tickCurrent,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              limitTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateBeforePartial = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );
      assert.isTrue(
        tickStateBeforePartial.ordersAmount.toString() ===
          ORDER_AMOUNT.toString(),
        "tickState.ordersAmount should be equal to ORDER_AMOUNT"
      );
      assert.isTrue(
        tickStateBeforePartial.partFilledOrdersRemaining.toString() === "0",
        "tickState.partFilledOrdersRemaining should be equal to 0"
      );
      assert.isTrue(
        tickStateBeforePartial.unfilledRatioX64.toString() === "0",
        "tickState.unfilledRatioX64 should be 0 before any partial fill"
      );

      // Check pool price and tickCurrent
      const poolBeforePartial = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitBeforePartial =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolBeforePartial.tickCurrent,
        limitTick,
        "tickCurrent should be limitTick when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolBeforePartial.sqrtPriceX64.toString(),
        expectedSqrtAtLimitBeforePartial.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      // Limit order is partially filled
      await instructions.swapV2(
        {
          owner: user,
          ammConfig: poolStateData.ammConfig,
          poolState: poolStateLocal,
          inputVaultMint: inputVaultMint,
          outputVaultMint: outputVaultMint,
          amount: SWAP_AMOUNT,
          otherAmountThreshold: new anchor.BN(10_000_000_000),
          sqrtPriceLimitX64: new anchor.BN(0),
          isBaseInput: false,
          remainingAccounts: [
            {
              pubkey: TickUtils.getTickArrayAddressByTick(
                program.programId,
                poolStateLocal,
                limitTick,
                tickSpacing
              ),
              isWritable: true,
              isSigner: false,
            },
          ],
        },
        { skipPreflight: true }
      );

      // Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateAfterPartial = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );

      assert.isTrue(
        tickStateAfterPartial.ordersAmount.toString() === "0",
        "tickState.ordersAmount should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterPartial.partFilledOrdersRemaining.toString() ===
          ORDER_AMOUNT.sub(SWAP_AMOUNT).toString(),
        "tickState.partFilledOrdersRemaining should be equal to ORDER_AMOUNT - SWAP_AMOUNT"
      );
      assert.isTrue(
        !tickStateAfterPartial.unfilledRatioX64.isZero(),
        "tickState.unfilledRatioX64 should be non-zero after partial fill"
      );

      // Check pool price and tickCurrent
      const poolAfterPartial = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitAfterPartial =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolAfterPartial.tickCurrent,
        limitTick,
        "tickCurrent should be limitTick + 1 when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolAfterPartial.sqrtPriceX64.toString(),
        expectedSqrtAtLimitAfterPartial.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      // Limit order is fully filled
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        amount: ORDER_AMOUNT.sub(SWAP_AMOUNT),
        otherAmountThreshold: new anchor.BN(10_000_000_000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              limitTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      //  Fetch tick array and read the tickState at the limit order tick; verify fields
      let tickStateAfterFull = await getTickStateByTick(
        program,
        poolStateLocal,
        limitTick,
        tickSpacing
      );

      assert.isTrue(
        tickStateAfterFull.ordersAmount.toString() === "0",
        "tickState.ordersAmount should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterFull.partFilledOrdersRemaining.toString() === "0",
        "tickState.partFilledOrdersRemaining should be equal to 0"
      );
      assert.isTrue(
        tickStateAfterFull.unfilledRatioX64.isZero(),
        "tickState.unfilledRatioX64 should collapse to 0 after full fill"
      );

      // Check pool price and tickCurrent
      const poolAfterFull = await program.account.poolState.fetch(
        poolStateLocal
      );
      const expectedSqrtAtLimitAfterFull =
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick);

      assert.equal(
        poolAfterFull.tickCurrent,
        limitTick - 1,
        "tickCurrent should be limitTick - 1 when price lands exactly on limit tick for zero_for_one"
      );
      assert.equal(
        poolAfterFull.sqrtPriceX64.toString(),
        expectedSqrtAtLimitAfterFull.toString(),
        "pool sqrtPrice should equal sqrt(price at limit tick)"
      );

      const poolNewLiquidity = poolAfterPartial.liquidity.add(
        tickStateAfterFull.liquidityNet.neg()
      );

      assert.equal(
        poolNewLiquidity.toString(),
        poolAfterFull.liquidity.toString(),
        "poolNewLiquidity should equal poolAfterFull.liquidity"
      );

      //  Settle the limit order and verify filledAmount
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const orderState = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(
        orderState.filledAmount.toString(),
        ORDER_AMOUNT.toString(),
        "limit order filledAmount should equal ORDER_AMOUNT after full fill and settlement"
      );
    });
  });
});
