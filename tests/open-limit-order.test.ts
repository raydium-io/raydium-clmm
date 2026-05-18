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

describe("limit_order_test", () => {
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

  describe("openLimitOrder", () => {
    it("Successfully opens a limit order (zero_for_one=true)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        0
      );

      assert.isTrue(
        validTick > tickCurrent,
        "Valid tick should be > current tick"
      );
      assert.equal(
        validTick % tickSpacing,
        0,
        "Valid tick should be multiple of tickSpacing"
      );

      const result = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });

      assert.ok(result.signature, "Transaction should succeed");
    });

    it("Successfully opens a limit order (zero_for_one=false)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        false,
        0
      );

      assert.isTrue(
        validTick < tickCurrent,
        "Valid tick should be < current tick"
      );
      assert.equal(
        validTick % tickSpacing,
        0,
        "Valid tick should be multiple of tickSpacing"
      );

      const result = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: false,
        amount: new anchor.BN(1_000_000),
      });

      assert.ok(result.signature, "Transaction should succeed");
    });

    it("Fails when tick_index <= tick_current (zero_for_one=true)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;

      // Use tick_current or less (should fail)
      const invalidTick = Math.floor(tickCurrent / tickSpacing) * tickSpacing;

      try {
        await instructions.openLimitOrder({
          owner: user,
          poolState: poolState,
          tickIndex: invalidTick,
          zeroForOne: true,
          amount: new anchor.BN(1_000_000),
        });
        assert.fail("Should have thrown an error");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "InvalidTickIndex",
          "Should throw InvalidTickIndex error"
        );
      }
    });

    it("Fails when tick_index >= tick_current (zero_for_one=false)", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;

      let invalidTick =
        Math.floor((tickCurrent + 1) / tickSpacing) * tickSpacing;
      if (invalidTick <= tickCurrent) invalidTick += tickSpacing;
      assert.isTrue(
        invalidTick >= tickCurrent,
        "invalidTick should be >= current for zero_for_one=false"
      );

      try {
        await instructions.openLimitOrder({
          owner: user,
          poolState: poolState,
          tickIndex: invalidTick,
          zeroForOne: false,
          amount: new anchor.BN(1_000_000),
        });
        assert.fail("Should have thrown InvalidTickIndex");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "InvalidTickIndex",
          "Should throw InvalidTickIndex when tick >= current for zero_for_one=false"
        );
      }
    });

    it("Fails when tick_index is not a multiple of tick_spacing", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;

      // Create a tick that is not a multiple of tickSpacing
      const invalidTick = tickCurrent + tickSpacing + 1; // +1 makes it not divisible

      try {
        await instructions.openLimitOrder({
          owner: user,
          poolState: poolState,
          tickIndex: invalidTick,
          zeroForOne: true,
          amount: new anchor.BN(1_000_000),
        });
        assert.fail("Should have thrown an error");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "TickAndSpacingNotMatch",
          "Should throw TickAndSpacingNotMatch error"
        );
      }
    });

    it("Fails when amount is zero", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        3,
      );

      try {
        await instructions.openLimitOrder({
          owner: user,
          poolState: poolState,
          tickIndex: validTick,
          zeroForOne: true,
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

    it("Fails when amount exceeds maximum", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        4,
      );
      const maxAmount = new anchor.BN("18446744073709551615"); // u64::MAX

      try {
        await instructions.openLimitOrder({
          owner: user,
          poolState: poolState,
          tickIndex: validTick,
          zeroForOne: true,
          amount: maxAmount,
        });
        assert.fail("Should have thrown InvalidLimitOrderAmount");
      } catch (err: any) {
        assert.include(
          err.toString(),
          "InvalidLimitOrderAmount",
          "Should throw InvalidLimitOrderAmount error"
        );
      }
    });

    it("Opens limit order and verifies tick array bitmap is set", async () => {
      const poolStateData = await program.account.poolState.fetch(poolState);
      const tickSpacing = poolStateData.tickSpacing;
      const tickCurrent = poolStateData.tickCurrent;
      const validTick = getValidTickForLimitOrder(
        tickCurrent,
        tickSpacing,
        true,
        2,
      );
      const validTickInExtension =
        getValidTickForLimitOrder(tickCurrent, tickSpacing, true, 5) +
        512 * 60 * tickSpacing;

      const result = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTick,
        zeroForOne: true,
        amount: new anchor.BN(1_000_000),
      });
      assert.ok(result.signature, "Transaction should succeed");
      const bit = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolState,
        validTick,
        tickSpacing
      );
      assert.strictEqual(bit, 1, "Tick array bitmap bit should be set");

      const bitInExtensionBefore = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolState,
        validTickInExtension,
        tickSpacing
      );
      assert.strictEqual(
        bitInExtensionBefore,
        0,
        "Tick array bitmap bit should be 0 in extension"
      );

      const result2 = await instructions.openLimitOrder({
        owner: user,
        poolState: poolState,
        tickIndex: validTickInExtension,
        zeroForOne: true,
        amount: new anchor.BN(1_000),
      });
      assert.ok(result2.signature, "Second open should succeed");
      const bitInExtension = await getTickArrayBitmapBit(
        program,
        instructions.pda,
        poolState,
        validTickInExtension,
        tickSpacing
      );
      assert.strictEqual(
        bitInExtension,
        1,
        "Tick array bitmap bit should be set in extension"
      );
    });
  });
});
