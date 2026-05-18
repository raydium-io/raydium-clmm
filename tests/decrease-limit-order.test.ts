import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import {
  getTickArrayBitmapBit,
  getValidTickForLimitOrder,
  cleanupAllLimitOrders,
} from "./utils/util";
import { getAssociatedTokenAddressSync, getAccount } from "@solana/spl-token";
import { TickUtils } from "@raydium-io/raydium-sdk-v2";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
const user = provider.wallet.payer;
const setup = new TestSetup(program, user);
const instructions = new InstructionHelper(program);

let poolState: anchor.web3.PublicKey;

describe("decrease_limit_order_test", () => {
  before(async () => {
    await setup.initialize();
    // Clean up any existing limit orders before running tests
    await cleanupAllLimitOrders(program, instructions, user);
    poolState = await setup.createPool(0); // Create pool at tick 0
  });

  describe("decreaseLimitOrder", () => {
    it("decreases partially and keeps order open", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        0,
      );

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      const before = await program.account.limitOrderState.fetch(limitOrder);

      await instructions.decreaseLimitOrder({
        owner: user,
        poolState: poolState,
        limitOrder: limitOrder,
        amount: new anchor.BN(400_000),
        amountMin: new anchor.BN(0),
      });

      const after = await program.account.limitOrderState.fetch(limitOrder);
      const expected = before.totalAmount.sub(new anchor.BN(400_000));
      assert.isTrue(
        after.totalAmount.eq(expected),
        "totalAmount should equal before - decreased amount"
      );
    });

    it("fails when amount is zero", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        1,
      );

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(500_000),
      });

      try {
        await instructions.decreaseLimitOrder({
          owner: user,
          poolState: poolState,
          limitOrder: limitOrder,
          amount: new anchor.BN(0),
          amountMin: new anchor.BN(0),
        });
        assert.fail("Should have thrown ZeroAmountSpecified");
      } catch (e: any) {
        assert.include(e.toString(), "ZeroAmountSpecified");
      }
    });

    it("decreases more than unfilled: closes order and clears bitmap if last on tick", async () => {
      const poolLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(poolLocal);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        4,
      );

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(123_456),
      });

      const bitBefore = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolLocal,
        validTick,
        tickSpacing
      );
      assert.strictEqual(bitBefore, 1, "bitmap should be set before decrease");

      await instructions.decreaseLimitOrder({
        owner: user,
        poolState: poolLocal,
        limitOrder: limitOrder,
        amount: new anchor.BN(9_999_999_999),
        amountMin: new anchor.BN(0),
      });

      // Verify unfilled amount is now zero
      const afterDecrease = await program.account.limitOrderState.fetch(
        limitOrder
      );
      const unfilledAfterDecrease = afterDecrease.totalAmount.sub(
        afterDecrease.filledAmount
      );
      assert.equal(
        unfilledAfterDecrease.toString(),
        "0",
        "unfilled amount should be zero after full decrease"
      );

      await instructions.closeLimitOrder({
        owner: user,
        limitOrder: limitOrder,
      });

      let closed = false;
      try {
        await program.account.limitOrderState.fetch(limitOrder);
      } catch (_e) {
        closed = true;
      }
      assert.isTrue(
        closed,
        "limit order account should be closed after closeLimitOrder"
      );

      const bitAfter = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolLocal,
        validTick,
        tickSpacing
      );
      assert.strictEqual(
        bitAfter,
        0,
        "bitmap should be cleared when last order removed"
      );
    });

    it("one_for_zero base output: swap half, settle, then decrease and verify amounts", async () => {
      const poolStateLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(
        poolStateLocal
      );
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        2,
      );

      const ORDER_AMOUNT = new anchor.BN(1_000_000);
      const HALF = new anchor.BN(500_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      // Pre-swap balances (one_for_zero, base output => output is token0, input is token1)
      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;
      const ata0 = getAssociatedTokenAddressSync(token0, user.publicKey);
      const ata1 = getAssociatedTokenAddressSync(token1, user.publicKey);
      const pre0 = await getAccount(provider.connection, ata0);
      const pre1 = await getAccount(provider.connection, ata1);

      // Execute swap: base output, amount = HALF
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: HALF,
        otherAmountThreshold: new anchor.BN(1000000000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              validTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Post-swap balances
      const post0 = await getAccount(provider.connection, ata0);
      const post1 = await getAccount(provider.connection, ata1);

      const delta0 = new anchor.BN(post0.amount.toString()).sub(
        new anchor.BN(pre0.amount.toString())
      );
      const delta1 = new anchor.BN(post1.amount.toString()).sub(
        new anchor.BN(pre1.amount.toString())
      );

      // Base output: output amount should equal HALF (fees charged on input side)
      assert.isTrue(
        delta0.eq(HALF),
        "output token0 received should equal HALF"
      );
      // Input should be negative (spent); ensure magnitude >= 0
      assert.isTrue(
        delta1.lte(new anchor.BN(0)),
        "input token1 should be spent"
      );

      const afterSwap = await program.account.limitOrderState.fetch(limitOrder);
      const unfilledAfterSwap = afterSwap.totalAmount.sub(
        afterSwap.filledAmount
      );
      assert.isTrue(
        unfilledAfterSwap.eq(ORDER_AMOUNT),
        "unfilled should equal ORDER_AMOUNT before settle (filledAmount not updated yet)"
      );
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const DEC = new anchor.BN(400_000);
      await instructions.decreaseLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder,
        amount: DEC,
        amountMin: new anchor.BN(0),
      });

      const afterDec = await program.account.limitOrderState.fetch(limitOrder);
      const expectedTotal = new anchor.BN(600_000);
      assert.equal(
        afterDec.totalAmount.toString(),
        expectedTotal.toString(),
        "totalAmount should decrease by DEC"
      );
      const unfilledAfterDec = afterDec.totalAmount.sub(afterDec.filledAmount);
      const expectedUnfilledAfterDec = new anchor.BN(100_000);
      assert.equal(
        unfilledAfterDec.toString(),
        expectedUnfilledAfterDec.toString(),
        "unfilled should reduce by DEC after decrease"
      );
    });

    it("fully fills via swap: settle, then decrease and close", async () => {
      const poolStateLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(
        poolStateLocal
      );
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        3,
      );

      const ORDER_AMOUNT = new anchor.BN(800_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      const bitBefore = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolStateLocal,
        validTick,
        tickSpacing
      );
      assert.strictEqual(bitBefore, 1, "bitmap should be set after open");

      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: ORDER_AMOUNT,
        otherAmountThreshold: new anchor.BN(1000000000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolStateLocal,
              validTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      const bitAfterSwap = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolStateLocal,
        validTick,
        tickSpacing
      );
      assert.strictEqual(
        bitAfterSwap,
        0,
        "bitmap should be cleared after full fill via swap"
      );

      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      await instructions.decreaseLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder,
        amount: new anchor.BN(1),
        amountMin: new anchor.BN(0),
      });

      // Verify order still exists but unfilled is 0
      const afterDecrease = await program.account.limitOrderState.fetch(
        limitOrder
      );
      const unfilledAfterDecrease = afterDecrease.totalAmount.sub(
        afterDecrease.filledAmount
      );
      assert.equal(
        unfilledAfterDecrease.toString(),
        "0",
        "unfilled amount should be zero after decrease on fully filled order"
      );

      // Close the order account since unfilled amount is zero
      await instructions.closeLimitOrder({
        owner: user,
        limitOrder,
      });

      // Order accounts should be closed; fetch should fail
      let closed = false;
      try {
        await program.account.limitOrderState.fetch(limitOrder);
      } catch (_e) {
        closed = true;
      }
      assert.isTrue(
        closed,
        "limit order should be closed after closeLimitOrder"
      );
    });
  });
});
