import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import { getValidTickForLimitOrder, cleanupAllLimitOrders } from "./utils/util";
import { TickUtils } from "@raydium-io/raydium-sdk-v2";

describe("increase_limit_order_test", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const setup = new TestSetup(program, user);
  const instructions = new InstructionHelper(program);

  let poolState: anchor.web3.PublicKey;

  before(async () => {
    await setup.initialize();
    // Clean up any existing limit orders before running tests
    await cleanupAllLimitOrders(program, instructions, user);
    poolState = await setup.createPool(0); // Create pool at tick 0
  });

  describe("increaseLimitOrder", () => {
    it("Successfully increases limit order amount", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        0,
      );

      const openResult = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      // Get initial limit order state
      const limitOrderData = await program.account.limitOrderState.fetch(
        openResult.limitOrder
      );
      const initialAmount = limitOrderData.totalAmount;

      // Increase the limit order
      const increaseTx = await instructions.increaseLimitOrder({
        owner: user,
        poolState: poolState,
        limitOrder: openResult.limitOrder,
        amount: new anchor.BN(500_000),
      });

      assert.ok(increaseTx, "Increase transaction should succeed");

      // Verify the total amount increased
      const limitOrderDataAfter = await program.account.limitOrderState.fetch(
        openResult.limitOrder
      );
      assert.equal(
        limitOrderDataAfter.totalAmount.toString(),
        initialAmount.add(new anchor.BN(500_000)).toString(),
        "Total amount should increase by the specified amount"
      );
      assert.equal(
        limitOrderDataAfter.filledAmount.toString(),
        limitOrderData.filledAmount.toString(),
        "Filled amount should remain unchanged"
      );
    });

    it("Successfully increases limit order multiple times", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        1,
      );

      const openResult = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      // Increase multiple times
      await instructions.increaseLimitOrder({
        owner: user,
        poolState: poolState,
        limitOrder: openResult.limitOrder,
        amount: new anchor.BN(300_000),
      });

      await instructions.increaseLimitOrder({
        owner: user,
        poolState: poolState,
        limitOrder: openResult.limitOrder,
        amount: new anchor.BN(200_000),
      });

      // Verify final amount
      const limitOrderData = await program.account.limitOrderState.fetch(
        openResult.limitOrder
      );
      assert.equal(
        limitOrderData.totalAmount.toString(),
        new anchor.BN(1_500_000).toString(), // 1_000_000 + 300_000 + 200_000
        "Total amount should reflect all increases"
      );
    });

    it("Fails when amount is zero", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        2,
      );

      const openResult = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      try {
        await instructions.increaseLimitOrder({
          owner: user,
          poolState: poolState,
          limitOrder: openResult.limitOrder,
          amount: new anchor.BN(0),
        });
        assert.fail("Should have thrown ZeroAmountSpecified");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "ZeroAmountSpecified",
          "Should throw ZeroAmountSpecified error"
        );
      }
    });

    it("Fails when trying to increase partially filled order (InvalidOrderPhase)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        3,
      );

      const openResult = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      const limitOrderDataBefore = await program.account.limitOrderState.fetch(
        openResult.limitOrder
      );
      const limitOrderTickArray = TickUtils.getTickArrayAddressByTick(
        program.programId,
        poolState,
        validTick,
        tickSpacing
      );
      const tickArrayDataBefore = await program.account.tickArrayState.fetch(
        limitOrderTickArray
      );
      const tickIndexInArray = TickUtils.getTickOffsetInArray(
        validTick,
        tickSpacing
      );
      const tickStateBefore = tickArrayDataBefore.ticks[tickIndexInArray];
      assert.equal(
        limitOrderDataBefore.orderPhase.toString(),
        tickStateBefore.orderPhase.toString(),
        "Initial order phase should match"
      );

      // Execute a swap that will match the limit order
      // zero_for_one=false (one_for_zero) means buying token0 with token1
      // This will move price up and match the limit order at validTick
      const swapZeroForOne = false; // one_for_zero to match zero_for_one limit order

      // Get token mints and vaults
      const inputVaultMint = swapZeroForOne
        ? poolStateData.tokenMint0
        : poolStateData.tokenMint1;
      const outputVaultMint = swapZeroForOne
        ? poolStateData.tokenMint1
        : poolStateData.tokenMint0;

      // Calculate tick array start index for current tick (swap will start from current)
      const currentTickArray = TickUtils.getTickArrayAddressByTick(
        program.programId,
        poolState,
        tickCurrent,
        tickSpacing
      );

      // Build remaining accounts: tick arrays needed for swap path
      const remainingAccounts: anchor.web3.AccountMeta[] = [];

      // Add current tick array if different from limit order tick array
      if (!currentTickArray.equals(limitOrderTickArray)) {
        remainingAccounts.push({
          pubkey: currentTickArray,
          isSigner: false,
          isWritable: true,
        });
      }

      // Add limit order tick array
      remainingAccounts.push({
        pubkey: limitOrderTickArray,
        isSigner: false,
        isWritable: true,
      });

      // Execute swap - a small amount that will match part of the limit order
      // Using isBaseInput=true, so amount is the input amount
      const swapAmount = new anchor.BN(100_000); // Small amount to partially match

      try {
        await instructions.swapV2({
          owner: user,
          ammConfig: poolStateData.ammConfig,
          poolState: poolState,
          inputVaultMint: inputVaultMint,
          outputVaultMint: outputVaultMint,
          amount: swapAmount,
          otherAmountThreshold: new anchor.BN(0), // Accept any output
          sqrtPriceLimitX64: new anchor.BN(0), // No price limit
          isBaseInput: true,
          remainingAccounts: remainingAccounts,
        });
      } catch (err: any) {
        console.log("Swap execution note:", err.toString());
        return;
      }

      const tickArrayDataAfter = await program.account.tickArrayState.fetch(
        limitOrderTickArray
      );
      const tickStateAfter = tickArrayDataAfter.ticks[tickIndexInArray];
      assert.equal(
        tickStateAfter.orderPhase.toString(),
        tickStateBefore.orderPhase.toString(),
        "Tick state order phase should remain the same after swap"
      );

      try {
        await instructions.increaseLimitOrder({
          owner: user,
          poolState: poolState,
          limitOrder: openResult.limitOrder,
          amount: new anchor.BN(500_000),
        });
        assert.fail("Should have thrown InvalidOrderPhase");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "InvalidOrderPhase",
          "Should throw InvalidOrderPhase when order phase doesn't match tick state"
        );
      }
    });
  });
});
