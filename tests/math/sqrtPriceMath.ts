import { MaxUint128 } from '@cykura/sdk-core'
import JSBI from 'jsbi'
import invariant from 'tiny-invariant'
import { ONE, ZERO, Q64, U64Resolution } from '../constants'
import { FullMath } from './fullMath'

function multiplyIn128(x: JSBI, y: JSBI): JSBI {
  const product = JSBI.multiply(x, y)
  return JSBI.bitwiseAnd(product, MaxUint128)
}

function addIn128(x: JSBI, y: JSBI): JSBI {
  const sum = JSBI.add(x, y)
  return JSBI.bitwiseAnd(sum, MaxUint128)
}

export abstract class SqrtPriceMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static getAmount0Delta(sqrtRatioAX64: JSBI, sqrtRatioBX64: JSBI, liquidity: JSBI, roundUp: boolean): JSBI {
    if (JSBI.greaterThan(sqrtRatioAX64, sqrtRatioBX64)) {
      ;[sqrtRatioAX64, sqrtRatioBX64] = [sqrtRatioBX64, sqrtRatioAX64]
    }

    const numerator1 = JSBI.leftShift(liquidity, U64Resolution)
    const numerator2 = JSBI.subtract(sqrtRatioBX64, sqrtRatioAX64)

    invariant(JSBI.greaterThan(sqrtRatioAX64, ZERO), 'SQRTA64_GT_0')

    return roundUp
      ? FullMath.mulDivRoundingUp(FullMath.mulDivCeil(numerator1, numerator2, sqrtRatioBX64), ONE, sqrtRatioAX64)
      : JSBI.divide(FullMath.mulDivFloor(numerator1, numerator2, sqrtRatioBX64), sqrtRatioAX64)
  }

  public static getAmount1Delta(sqrtRatioAX32: JSBI, sqrtRatioBX32: JSBI, liquidity: JSBI, roundUp: boolean): JSBI {
    if (JSBI.greaterThan(sqrtRatioAX32, sqrtRatioBX32)) {
      ;[sqrtRatioAX32, sqrtRatioBX32] = [sqrtRatioBX32, sqrtRatioAX32]
    }

    return roundUp
      ? FullMath.mulDivCeil(liquidity, JSBI.subtract(sqrtRatioBX32, sqrtRatioAX32), Q64)
      : FullMath.mulDivFloor(liquidity, JSBI.subtract(sqrtRatioBX32, sqrtRatioAX32), Q64)
  }

  public static getNextSqrtPriceFromInput(sqrtPX32: JSBI, liquidity: JSBI, amountIn: JSBI, zeroForOne: boolean): JSBI {
    invariant(JSBI.greaterThan(sqrtPX32, ZERO))
    invariant(JSBI.greaterThan(liquidity, ZERO))

    return zeroForOne
      ? this.getNextSqrtPriceFromAmount0RoundingUp(sqrtPX32, liquidity, amountIn, true)
      : this.getNextSqrtPriceFromAmount1RoundingDown(sqrtPX32, liquidity, amountIn, true)
  }

  public static getNextSqrtPriceFromOutput(
    sqrtPX32: JSBI,
    liquidity: JSBI,
    amountOut: JSBI,
    zeroForOne: boolean
  ): JSBI {
    invariant(JSBI.greaterThan(sqrtPX32, ZERO))
    invariant(JSBI.greaterThan(liquidity, ZERO))

    return zeroForOne
      ? this.getNextSqrtPriceFromAmount1RoundingDown(sqrtPX32, liquidity, amountOut, false)
      : this.getNextSqrtPriceFromAmount0RoundingUp(sqrtPX32, liquidity, amountOut, false)
  }

  private static getNextSqrtPriceFromAmount0RoundingUp(
    sqrtPriceX64: JSBI,
    liquidity: JSBI,
    amount: JSBI,
    add: boolean
  ): JSBI {
    if (JSBI.equal(amount, ZERO)) return sqrtPriceX64
    const numerator1 = JSBI.leftShift(liquidity, U64Resolution)

    if (add) {
      let product = multiplyIn128(amount, sqrtPriceX64)
      const denominator = addIn128(numerator1, product)
      if (JSBI.greaterThanOrEqual(denominator, numerator1)) {
        return FullMath.mulDivCeil(numerator1, sqrtPriceX64, denominator)
      }

      return FullMath.mulDivRoundingUp(numerator1, ONE, JSBI.add(JSBI.divide(numerator1, sqrtPriceX64), amount))
    } else {
      let product = multiplyIn128(amount, sqrtPriceX64)

      // invariant(JSBI.equal(JSBI.divide(product, amount), sqrtPX32))
      invariant(JSBI.greaterThan(numerator1, product))
      const denominator = JSBI.subtract(numerator1, product)
      return FullMath.mulDivCeil(numerator1, sqrtPriceX64, denominator)
    }
  }

  private static getNextSqrtPriceFromAmount1RoundingDown(
    sqrtPriceX64: JSBI,
    liquidity: JSBI,
    amount: JSBI,
    add: boolean
  ): JSBI {
    if (add) {
      return JSBI.add(sqrtPriceX64, JSBI.divide(JSBI.leftShift(amount, U64Resolution), liquidity))
    } else {
      const quotient =  FullMath.mulDivRoundingUp(JSBI.leftShift(amount, U64Resolution), ONE, liquidity)
      invariant(JSBI.greaterThan(sqrtPriceX64, quotient))
      return JSBI.subtract(sqrtPriceX64, quotient)
    }
  }
}
