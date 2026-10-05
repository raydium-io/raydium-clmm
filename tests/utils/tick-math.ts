import BN from "bn.js";
import { PublicKey } from "@solana/web3.js";

// Ported from `programs/amm/src/libraries/tick_math.rs` rather than imported from
// `@raydium-io/raydium-sdk-v2`. The suite used to take this from the SDK and broke silently
// when the SDK renamed `TickArrayUtil`/`TickUtil` to `TickUtils`/`SqrtPriceMath`: every call
// site was reading a method off `undefined`, and 33 tests failed with useless messages.
// These are a handful of pure functions that must agree with the on-chain program, not with
// an SDK release, so they live here and are pinned by golden values in tick-math.test.ts.

export const MIN_TICK = -443636;
export const MAX_TICK = 443636;
export const TICK_ARRAY_SIZE = 60;

const Q64 = new BN(1).shln(64);
const U128_MAX = new BN(1).shln(128).subn(1);

// 1.0001^(2^i) in Q64.64, for i = 0..18 - the same table as the Rust.
const BIT_RATIOS: string[] = [
  "fffcb933bd6fb800",
  "fff97272373d4000",
  "fff2e50f5f657000",
  "ffe5caca7e10f000",
  "ffcb9843d60f7000",
  "ff973b41fa98e800",
  "ff2ea16466c9b000",
  "fe5dee046a9a3800",
  "fcbe86c7900bb000",
  "f987a7253ac65800",
  "f3392b0822bb6000",
  "e7159475a2caf000",
  "d097f3bdfd2f2000",
  "a9f746462d9f8000",
  "70d869a156f31c00",
  "31be135f97ed3200",
  "9aa508b5b85a500",
  "5d6af8dedc582c",
  "2216e584f5fa",
];

/// `get_sqrt_price_at_tick`: sqrt(1.0001^tick) as a Q64.64 fixed point number.
export function getSqrtPriceX64FromTick(tick: number): BN {
  if (tick < MIN_TICK || tick > MAX_TICK) {
    throw new Error(`tick ${tick} out of range [${MIN_TICK}, ${MAX_TICK}]`);
  }
  const absTick = Math.abs(tick);

  let ratio = absTick & 0x1 ? new BN(BIT_RATIOS[0], 16) : Q64.clone();
  for (let i = 1; i < BIT_RATIOS.length; i++) {
    if (absTick & (1 << i)) {
      ratio = ratio.mul(new BN(BIT_RATIOS[i], 16)).shrn(64);
    }
  }
  // The table is 1.0001^-x, so a positive tick is the reciprocal.
  return tick > 0 ? U128_MAX.div(ratio) : ratio;
}

/// The start index of the tick array a tick falls in. Negative ticks round away from zero,
/// which is what `TickArrayState::get_array_start_index` does on chain.
export function getTickArrayStartIndex(
  tickIndex: number,
  tickSpacing: number
): number {
  const ticksInArray = tickSpacing * TICK_ARRAY_SIZE;
  // `Math.trunc`, not `Math.floor`: Rust's `/` truncates toward zero and the program then
  // compensates for negatives itself. Flooring here would subtract a whole array twice.
  let start = Math.trunc(tickIndex / ticksInArray);
  if (tickIndex < 0 && tickIndex % ticksInArray !== 0) {
    start -= 1;
  }
  return start * ticksInArray;
}

/// A tick's slot within its tick array.
export function getTickOffsetInArray(
  tickIndex: number,
  tickSpacing: number
): number {
  const start = getTickArrayStartIndex(tickIndex, tickSpacing);
  return Math.floor((tickIndex - start) / tickSpacing);
}

// Plain constants; there is no reason to pull an SDK in for these.
export const MEMO_PROGRAM_ID = new PublicKey(
  "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
);
export const METADATA_PROGRAM_ID = new PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);
