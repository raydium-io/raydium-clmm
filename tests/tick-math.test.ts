import BN from "bn.js";
import { assert } from "chai";
import {
  getSqrtPriceX64FromTick,
  getTickArrayStartIndex,
  getTickOffsetInArray,
  MAX_TICK,
  MIN_TICK,
} from "./utils/tick-math";

// Generated from the on-chain `get_sqrt_price_at_tick` (programs/amm/src/libraries/tick_math.rs).
// Regenerate by adding a `println!` test there if the Rust ever changes - these have to track
// the program, not an SDK release.
const GOLDEN: [number, string][] = [
  [-443636, "4295048016"],
  [-443600, "4302785677"],
  [-100000, "124324258983086206"],
  [-50000, "1514390236238525528"],
  [-10000, "11188795550325113405"],
  [-1000, "17547129613991882732"],
  [-100, "18354745142194513203"],
  [-1, "18445821805675395072"],
  [0, "18446744073709551616"],
  [1, "18447666387855957090"],
  [100, "18539204128674375874"],
  [1000, "19392480388906522465"],
  [10000, "30412779051186690180"],
  [50000, "224699260981858280811"],
  [100000, "2737055259402209284734"],
  [443600, "79084200902655943154329368525"],
  [443636, "79226673521066979257578248091"],
];

describe("tick math", () => {
  it("matches the on-chain get_sqrt_price_at_tick", () => {
    for (const [tick, expected] of GOLDEN) {
      assert.equal(
        getSqrtPriceX64FromTick(tick).toString(),
        expected,
        `tick ${tick}`
      );
    }
  });

  it("rejects ticks outside the program's range", () => {
    assert.throws(() => getSqrtPriceX64FromTick(MIN_TICK - 1));
    assert.throws(() => getSqrtPriceX64FromTick(MAX_TICK + 1));
  });

  it("rounds tick array starts away from zero for negative ticks", () => {
    // tickSpacing 10 => 600 ticks per array
    assert.equal(getTickArrayStartIndex(0, 10), 0);
    assert.equal(getTickArrayStartIndex(599, 10), 0);
    assert.equal(getTickArrayStartIndex(600, 10), 600);
    assert.equal(getTickArrayStartIndex(-1, 10), -600);
    assert.equal(getTickArrayStartIndex(-600, 10), -600);
    assert.equal(getTickArrayStartIndex(-601, 10), -1200);
  });

  it("places a tick at the right offset in its array", () => {
    assert.equal(getTickOffsetInArray(0, 10), 0);
    assert.equal(getTickOffsetInArray(590, 10), 59);
    assert.equal(getTickOffsetInArray(-600, 10), 0);
    assert.equal(getTickOffsetInArray(-10, 10), 59);
  });
});
