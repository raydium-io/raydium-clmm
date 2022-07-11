import JSBI from 'jsbi'
import { NEGATIVE_ONE, ZERO } from './constants'
import { BigintIsh } from '@cykura/sdk-core'
import { FullMath } from '../math/fullMath'
import {MaxUint64 } from '../math/constants'

export abstract class LiquidityMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static addDelta(x: JSBI, y: JSBI): JSBI {
    let z: JSBI
    if (JSBI.lessThan(y, ZERO)) {
      z = JSBI.subtract(x, JSBI.multiply(y, NEGATIVE_ONE))
    } else {
      z = JSBI.add(x, y)
    }
    return z
  }

  
/**
 * Returns an imprecise maximum amount of liquidity received for a given amount of token 0.
 * This function is available to accommodate LiquidityAmounts#getLiquidityForAmount0 in the v3 periphery,
 * which could be more precise by at least 32 bits by dividing by Q64 instead of Q96 in the intermediate step,
 * and shifting the subtracted ratio left by 32 bits. This imprecise calculation will likely be replaced in a future
 * v3 router contract.
 * @param sqrtRatioAX64 The price at the lower boundary
 * @param sqrtRatioBX64 The price at the upper boundary
 * @param amount0 The token0 amount
 * @returns liquidity for amount0, imprecise
 */
 public static maxLiquidityForAmount0Imprecise(sqrtRatioAX64: JSBI, sqrtRatioBX64: JSBI, amount0: BigintIsh): JSBI {
  if (JSBI.greaterThan(sqrtRatioAX64, sqrtRatioBX64)) {
    ;[sqrtRatioAX64, sqrtRatioBX64] = [sqrtRatioBX64, sqrtRatioAX64]
  }
  const intermediate = FullMath.mulDivFloor(sqrtRatioAX64, sqrtRatioBX64, MaxUint64)
  return FullMath.mulDivFloor(JSBI.BigInt(amount0), intermediate, JSBI.subtract(sqrtRatioBX64, sqrtRatioAX64))
}

/**
 * Returns a precise maximum amount of liquidity received for a given amount of token 0 by dividing by Q64 instead of Q96 in the intermediate step,
 * and shifting the subtracted ratio left by 32 bits.
 * @param sqrtRatioAX64 The price at the lower boundary
 * @param sqrtRatioBX64 The price at the upper boundary
 * @param amount0 The token0 amount
 * @returns liquidity for amount0, precise
 */
 public static maxLiquidityForAmount0Precise(sqrtRatioAX64: JSBI, sqrtRatioBX64: JSBI, amount0: BigintIsh): JSBI {
  if (JSBI.greaterThan(sqrtRatioAX64, sqrtRatioBX64)) {
    ;[sqrtRatioAX64, sqrtRatioBX64] = [sqrtRatioBX64, sqrtRatioAX64]
  }

  const numerator = JSBI.multiply(JSBI.multiply(JSBI.BigInt(amount0), sqrtRatioAX64), sqrtRatioBX64)
  const denominator = JSBI.multiply(MaxUint64, JSBI.subtract(sqrtRatioBX64, sqrtRatioAX64))

  return JSBI.divide(numerator, denominator)
}

/**
 * Computes the maximum amount of liquidity received for a given amount of token1
 * @param sqrtRatioAX64 The price at the lower tick boundary
 * @param sqrtRatioBX64 The price at the upper tick boundary
 * @param amount1 The token1 amount
 * @returns liquidity for amount1
 */
 public static maxLiquidityForAmount1(sqrtRatioAX64: JSBI, sqrtRatioBX64: JSBI, amount1: BigintIsh): JSBI {
  if (JSBI.greaterThan(sqrtRatioAX64, sqrtRatioBX64)) {
    ;[sqrtRatioAX64, sqrtRatioBX64] = [sqrtRatioBX64, sqrtRatioAX64]
  }
  return FullMath.mulDivFloor(JSBI.BigInt(amount1), MaxUint64, JSBI.subtract(sqrtRatioBX64, sqrtRatioAX64))
}

/**
 * Computes the maximum amount of liquidity received for a given amount of token0, token1,
 * and the prices at the tick boundaries.
 * @param sqrtRatioCurrentX64 the current price
 * @param sqrtRatioAX64 price at lower boundary
 * @param sqrtRatioBX64 price at upper boundary
 * @param amount0 token0 amount
 * @param amount1 token1 amount
 * @param useFullPrecision if false, liquidity will be maximized according to what the router can calculate,
 * not what core can theoretically support
 */
 public static maxLiquidityForAmounts(
  sqrtRatioCurrentX64: JSBI,
  sqrtRatioAX64: JSBI,
  sqrtRatioBX64: JSBI,
  amount0: BigintIsh,
  amount1: BigintIsh,
  useFullPrecision: boolean
): JSBI {
  if (JSBI.greaterThan(sqrtRatioAX64, sqrtRatioBX64)) {
    ;[sqrtRatioAX64, sqrtRatioBX64] = [sqrtRatioBX64, sqrtRatioAX64]
  }

  // trying this out?
  useFullPrecision = false
  const maxLiquidityForAmount0 = LiquidityMath.maxLiquidityForAmount0Imprecise

  if (JSBI.lessThanOrEqual(sqrtRatioCurrentX64, sqrtRatioAX64)) {
    return maxLiquidityForAmount0(sqrtRatioAX64, sqrtRatioBX64, amount0)
  } else if (JSBI.lessThan(sqrtRatioCurrentX64, sqrtRatioBX64)) {
    const liquidity0 = maxLiquidityForAmount0(sqrtRatioCurrentX64, sqrtRatioBX64, amount0)
    const liquidity1 = LiquidityMath.maxLiquidityForAmount1(sqrtRatioAX64, sqrtRatioCurrentX64, amount1)
    return JSBI.lessThan(liquidity0, liquidity1) ? liquidity0 : liquidity1
  } else {
    return LiquidityMath.maxLiquidityForAmount1(sqrtRatioAX64, sqrtRatioBX64, amount1)
  }
}

}