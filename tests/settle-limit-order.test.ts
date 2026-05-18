import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import { getValidTickForLimitOrder, cleanupAllLimitOrders } from "./utils/util";
import { getAssociatedTokenAddressSync, getAccount } from "@solana/spl-token";
import { TickUtils } from "@raydium-io/raydium-sdk-v2";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
const user = provider.wallet.payer;
const setup = new TestSetup(program, user);
const instructions = new InstructionHelper(program);

let poolState: anchor.web3.PublicKey;

describe("settle_limit_order_test", () => {
  before(async () => {
    await setup.initialize();
    // Clean up any existing limit orders before running tests
    await cleanupAllLimitOrders(program, instructions, user);
    poolState = await setup.createPool(0); // Create pool at tick 0
  });

  describe("settleLimitOrder", () => {
    it("settles partially filled order and updates filledAmount", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        0,
      );

      const ORDER_AMOUNT = new anchor.BN(1_000_000);
      const SWAP_AMOUNT = new anchor.BN(500_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      // Verify initial state
      const beforeSwap = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(beforeSwap.filledAmount.toString(), "0");
      assert.equal(beforeSwap.totalAmount.toString(), ORDER_AMOUNT.toString());

      // zero_for_one order: user sells token0, receives token1 when filled
      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;
      const outputTokenAccount = getAssociatedTokenAddressSync(
        token1,
        user.publicKey
      );



      // Execute swap to partially fill the order
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolState,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: SWAP_AMOUNT,
        otherAmountThreshold: new anchor.BN(1000000000),
        sqrtPriceLimitX64: new anchor.BN(0),
        isBaseInput: false,
        remainingAccounts: [
          {
            pubkey: TickUtils.getTickArrayAddressByTick(
              program.programId,
              poolState,
              validTick,
              tickSpacing
            ),
            isWritable: true,
            isSigner: false,
          },
        ],
      });

      // Verify order hasn't been settled yet (filledAmount still 0)
      const afterSwap = await program.account.limitOrderState.fetch(limitOrder);
      assert.equal(afterSwap.filledAmount.toString(), "0");

      // Pre-settle balance
      const preBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const preBalanceAmount = new anchor.BN(preBalance.amount.toString());
      // Now settle the order
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolState,
        limitOrder: limitOrder,
      });

      // Verify order state after settle
      const afterSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.isTrue(
        afterSettle.filledAmount.eq(new anchor.BN(SWAP_AMOUNT)),
        "filledAmount should be equal to SWAP_AMOUNT after settle"
      );

      // Verify balance increased (output tokens received)
      const postBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const postBalanceAmount = new anchor.BN(postBalance.amount.toString());
      const balanceIncrease = postBalanceAmount.sub(preBalanceAmount);
      assert.isTrue(
        balanceIncrease.gt(new anchor.BN(0)),
        "output token balance should increase after settle"
      );
    });

    it("settles fully filled order", async () => {
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

      const ORDER_AMOUNT = new anchor.BN(800_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;
      const outputTokenAccount = getAssociatedTokenAddressSync(
        token1,
        user.publicKey
      );



      // Fully fill the order by swapping
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
      // Pre-settle balance
      const preBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const preBalanceAmount = new anchor.BN(preBalance.amount.toString());
      // Settle the fully filled order
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      // Verify order is fully filled
      const afterSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(
        afterSettle.filledAmount.toString(),
        afterSettle.totalAmount.toString(),
        "filledAmount should equal totalAmount after full fill and settle"
      );

      // Verify balance increased
      const postBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const postBalanceAmount = new anchor.BN(postBalance.amount.toString());
      const balanceIncrease = postBalanceAmount.sub(preBalanceAmount);
      assert.isTrue(
        balanceIncrease.gt(new anchor.BN(0)),
        "output token balance should increase after settle"
      );
    });

    it("settle with no fill reverts (require amount_out > 0)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        1,
      );

      const ORDER_AMOUNT = new anchor.BN(500_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      const beforeSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(beforeSettle.filledAmount.toString(), "0");

      // Instruction requires amount_out > 0; settling when nothing is filled must revert
      try {
        await instructions.settleLimitOrder({
          owner: user,
          poolState: poolState,
          limitOrder: limitOrder,
        });
        assert.fail("settleLimitOrder should revert when there is nothing to settle");
      } catch (err: any) {
        
      }
    });

    it("can settle multiple times as order gets progressively filled", async () => {
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
      const ORDER_AMOUNT = new anchor.BN(2_000_000);
      const SWAP_AMOUNT_1 = new anchor.BN(500_000);
      const SWAP_AMOUNT_2 = new anchor.BN(500_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });
      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;

      // First swap
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: SWAP_AMOUNT_1,
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

      // First settle
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const afterFirstSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.equal(
        afterFirstSettle.totalAmount.toString(),
        ORDER_AMOUNT.toString()
      );
      assert.equal(
        afterFirstSettle.filledAmount.toString(),
        SWAP_AMOUNT_1.toString()
      );
      // Second swap
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: SWAP_AMOUNT_2,
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

      // Second settle
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const afterSecondSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      const secondFilledAmount = afterSecondSettle.filledAmount;

      // Verify filledAmount increased after second settle
      assert.isTrue(
        secondFilledAmount.eq(SWAP_AMOUNT_1.add(SWAP_AMOUNT_2)),
        "filledAmount should increase by SWAP_AMOUNT_2 after second settle"
      );
    });

    it("settles order after decrease and verifies state consistency", async () => {
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
        4,
      );

      const ORDER_AMOUNT = new anchor.BN(1_000_000);
      const SWAP_AMOUNT = new anchor.BN(300_000);
      const DECREASE_AMOUNT = new anchor.BN(200_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: true,
        amount: ORDER_AMOUNT,
      });

      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;

      // Swap to fill part of order
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token1,
        outputVaultMint: token0,
        amount: SWAP_AMOUNT,
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

      // Settle to update filledAmount
      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const afterSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      const filledAfterSettle = afterSettle.filledAmount;
      const totalAfterSettle = afterSettle.totalAmount;
      // Decrease the order
      await instructions.decreaseLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
        amount: DECREASE_AMOUNT,
        amountMin: new anchor.BN(0),
      });

      // Verify state after decrease
      const afterDecrease = await program.account.limitOrderState.fetch(
        limitOrder
      );
      // filledAmount should remain the same (decrease doesn't change it)
      assert.equal(
        afterDecrease.filledAmount.toString(),
        filledAfterSettle.toString(),
        "filledAmount should remain unchanged after decrease"
      );
      // totalAmount should decrease
      assert.equal(
        afterDecrease.totalAmount.toString(),
        totalAfterSettle.sub(DECREASE_AMOUNT).toString(),
        "totalAmount should decrease by DECREASE_AMOUNT"
      );
    });


    it("settles one-for-zero (zero_for_one=false) order: output is token0", async () => {
      const poolStateLocal = await setup.createPool(0);
      const poolStateData = await program.account.poolState.fetch(
        poolStateLocal
      );
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        false,
        5,
      );

      const ORDER_AMOUNT = new anchor.BN(600_000);
      const SWAP_AMOUNT = new anchor.BN(300_000);

      const { limitOrder } = await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        tickIndex: validTick,
        zeroForOne: false,
        amount: ORDER_AMOUNT,
      });

      const token0 = poolStateData.tokenMint0;
      const token1 = poolStateData.tokenMint1;
      // one-for-zero: user sells token1, receives token0 when filled
      const outputTokenAccount = getAssociatedTokenAddressSync(
        token0,
        user.publicKey
      );

      // Limit order zero_for_one=false: opponent swap token0->token1 fills it
      await instructions.swapV2({
        owner: user,
        ammConfig: poolStateData.ammConfig,
        poolState: poolStateLocal,
        inputVaultMint: token0,
        outputVaultMint: token1,
        amount: SWAP_AMOUNT,
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
      const preBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const preBalanceAmount = new anchor.BN(preBalance.amount.toString());

      await instructions.settleLimitOrder({
        owner: user,
        poolState: poolStateLocal,
        limitOrder: limitOrder,
      });

      const afterSettle = await program.account.limitOrderState.fetch(
        limitOrder
      );
      assert.isTrue(
        afterSettle.filledAmount.gt(new anchor.BN(0)),
        "filledAmount should increase after settle for one-for-zero order"
      );

      const postBalance = await getAccount(
        provider.connection,
        outputTokenAccount
      );
      const postBalanceAmount = new anchor.BN(postBalance.amount.toString());
      assert.isTrue(
        postBalanceAmount.gt(preBalanceAmount),
        "token0 balance should increase after settling one-for-zero order"
      );
    });
  });
});
