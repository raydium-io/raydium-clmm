import { MaxUint128 } from '@cykura/sdk-core'
import { BN } from '@project-serum/anchor'
import JSBI from 'jsbi'
import invariant from 'tiny-invariant'
import { ONE, ZERO } from '../constants'
import { msb as mostSignificantBit } from '../entities/bitmap'

const BIT_PRECISION = 16;
const LOG_B_2_X32 = "59543866431248";
const LOG_B_P_ERR_MARGIN_LOWER_X64 = "184467440737095516";
const LOG_B_P_ERR_MARGIN_UPPER_X64 = "15793534762490258745";

function mulShift(val: JSBI, mulBy: string): JSBI {
  return JSBI.signedRightShift(JSBI.multiply(val, JSBI.BigInt(mulBy)), JSBI.BigInt(64))
}

const Q32 = JSBI.exponentiate(JSBI.BigInt(2), JSBI.BigInt(32))

export abstract class TickMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  /**
   * The minimum tick that can be used on any pool.
   */
  public static MIN_TICK: number = -443636
  /**
   * The maximum tick that can be used on any pool.
   */
  public static MAX_TICK: number = -TickMath.MIN_TICK

  /**
   * The sqrt ratio corresponding to the minimum tick that could be used on any pool.
   */
  public static MIN_SQRT_RATIO: JSBI = JSBI.BigInt('4295048016')
  /**
   * The sqrt ratio corresponding to the maximum tick that could be used on any pool.
   */
  public static MAX_SQRT_RATIO: JSBI = JSBI.BigInt('79226673521066979257578248091')

  /**
   * Returns the sqrt ratio as a Q32.32 for the given tick. The sqrt ratio is computed as sqrt(1.0001)^tick
   * @param tick the tick for which to compute the sqrt ratio
   */
  public static getSqrtRatioAtTick(tick: number): JSBI {
    invariant(tick >= TickMath.MIN_TICK && tick <= TickMath.MAX_TICK && Number.isInteger(tick), 'TICK')
    const absTick: number = tick < 0 ? tick * -1 : tick

    let ratio: JSBI = (absTick & 0x1) != 0 ? JSBI.BigInt('0xfffcb933bd6fb800') : JSBI.BigInt('0x10000000000000000')
    if ((absTick & 0x2) != 0) ratio = mulShift(ratio, '0xfff97272373d4000')
    if ((absTick & 0x4) != 0) ratio = mulShift(ratio, '0xfff2e50f5f657000')
    if ((absTick & 0x8) != 0) ratio = mulShift(ratio, '0xffe5caca7e10f000')
    if ((absTick & 0x10) != 0) ratio = mulShift(ratio, '0xffcb9843d60f7000')
    if ((absTick & 0x20) != 0) ratio = mulShift(ratio, '0xff973b41fa98e800')
    if ((absTick & 0x40) != 0) ratio = mulShift(ratio, '0xff2ea16466c9b000')
    if ((absTick & 0x80) != 0) ratio = mulShift(ratio, '0xfe5dee046a9a3800')
    if ((absTick & 0x100) != 0) ratio = mulShift(ratio, '0xfcbe86c7900bb000')
    if ((absTick & 0x200) != 0) ratio = mulShift(ratio, '0xf987a7253ac65800')
    if ((absTick & 0x400) != 0) ratio = mulShift(ratio, '0xf3392b0822bb6000')
    if ((absTick & 0x800) != 0) ratio = mulShift(ratio, '0xe7159475a2caf000')
    if ((absTick & 0x1000) != 0) ratio = mulShift(ratio, '0xd097f3bdfd2f2000')
    if ((absTick & 0x2000) != 0) ratio = mulShift(ratio, '0xa9f746462d9f8000')
    if ((absTick & 0x4000) != 0) ratio = mulShift(ratio, '0x70d869a156f31c00')
    if ((absTick & 0x8000) != 0) ratio = mulShift(ratio, '0x31be135f97ed3200')
    if ((absTick & 0x10000) != 0) ratio = mulShift(ratio, '0x9aa508b5b85a500')
    if ((absTick & 0x20000) != 0) ratio = mulShift(ratio, '0x5d6af8dedc582c')
    if ((absTick & 0x40000) != 0) ratio = mulShift(ratio, '0x2216e584f5fa')

    if (tick > 0) ratio = JSBI.divide(MaxUint128, ratio)
    console.log("getSqrtRatioAtTick, tick: ",tick, "price: ", ratio.toString())
    return ratio
    // // back to Q32
    // return JSBI.greaterThan(JSBI.remainder(ratio, Q32), ZERO)
    //   ? JSBI.add(JSBI.divide(ratio, Q32), ONE)
    //   : JSBI.divide(ratio, Q32)
  }

  /**
   * Returns the tick corresponding to a given sqrt ratio, s.t. #getSqrtRatioAtTick(tick) <= sqrtRatioX32
   * and #getSqrtRatioAtTick(tick + 1) > sqrtRatioX32
   * @param sqrtRatioX64 the sqrt ratio as a Q64.64 for which to compute the tick
   */
  public static getTickAtSqrtRatio(sqrtRatioX64: JSBI): number {
    invariant(
      JSBI.greaterThanOrEqual(sqrtRatioX64, TickMath.MIN_SQRT_RATIO) &&
        JSBI.lessThan(sqrtRatioX64, TickMath.MAX_SQRT_RATIO),
      'SQRT_RATIO'
    )
    
    const sqrtPriceX64 = new BN(sqrtRatioX64.toString())
    console.log("getTickAtSqrtRatio sqrtPriceX64:",sqrtPriceX64.toString(), "new BN(TickMath.MAX_SQRT_RATIO): ", new BN(TickMath.MAX_SQRT_RATIO.toString()))
    if (sqrtPriceX64.gt(new BN(TickMath.MAX_SQRT_RATIO.toString())) || sqrtPriceX64.lt(new BN(TickMath.MIN_SQRT_RATIO.toString()))) {
      throw new Error("Provided sqrtPrice is not within the supported sqrtPrice range.");
    }

    const msb = sqrtPriceX64.bitLength() - 1;
    const adjustedMsb = new BN(msb - 64);
    const log2pIntegerX32 = signedShiftLeft(adjustedMsb, 32, 128);

    let bit = new BN("8000000000000000", "hex");
    let precision = 0;
    let log2pFractionX64 = new BN(0);

    let r = msb >= 64 ? sqrtPriceX64.shrn(msb - 63) : sqrtPriceX64.shln(63 - msb);

    while (bit.gt(new BN(0)) && precision < BIT_PRECISION) {
      r = r.mul(r);
      let rMoreThanTwo = r.shrn(127);
      r = r.shrn(63 + rMoreThanTwo.toNumber());
      log2pFractionX64 = log2pFractionX64.add(bit.mul(rMoreThanTwo));
      bit = bit.shrn(1);
      precision += 1;
    }

    const log2pFractionX32 = log2pFractionX64.shrn(32);

    const log2pX32 = log2pIntegerX32.add(log2pFractionX32);
    const logbpX64 = log2pX32.mul(new BN(LOG_B_2_X32));

    const tickLow = signedShiftRight(
      logbpX64.sub(new BN(LOG_B_P_ERR_MARGIN_LOWER_X64)),
      64,
      128
    ).toNumber();
    const tickHigh = signedShiftRight(
      logbpX64.add(new BN(LOG_B_P_ERR_MARGIN_UPPER_X64)),
      64,
      128
    ).toNumber();
 
    if (tickLow == tickHigh) {
      return tickLow;
    } else {
      const derivedTickHighSqrtPriceX64 = new BN(TickMath.getSqrtRatioAtTick(tickHigh).toString());
      console.log("tickLow:",tickLow,"tickHigh:",tickHigh,"derivedTickHighSqrtPriceX64:",derivedTickHighSqrtPriceX64.toString())
      if (derivedTickHighSqrtPriceX64.lte(sqrtPriceX64)) {
        return tickHigh;
      } else {
        return tickLow;
      }
    }
  }
}

function signedShiftLeft(n0: BN, shiftBy: number, bitWidth: number) {
  let twosN0 = n0.toTwos(bitWidth).shln(shiftBy);
  twosN0.imaskn(bitWidth + 1);
  return twosN0.fromTwos(bitWidth);
}

function signedShiftRight(n0: BN, shiftBy: number, bitWidth: number) {
  let twoN0 = n0.toTwos(bitWidth).shrn(shiftBy);
  twoN0.imaskn(bitWidth - shiftBy + 1);
  return twoN0.fromTwos(bitWidth - shiftBy);
}
