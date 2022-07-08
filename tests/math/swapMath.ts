import JSBI from 'jsbi'
import { FeeAmount } from '../entities/fee'
import { NEGATIVE_ONE, ZERO } from '../constants'
import { FullMath } from './fullMath'
import { SqrtPriceMath } from './sqrtPriceMath'

const MAX_FEE = JSBI.exponentiate(JSBI.BigInt(10), JSBI.BigInt(6))

export abstract class SwapMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static computeSwapStep(
    sqrtRatioCurrentX64: JSBI,
    sqrtRatioTargetX64: JSBI,
    liquidity: JSBI,
    amountRemaining: JSBI,
    feePips: FeeAmount
  ): [JSBI, JSBI, JSBI, JSBI] {
    const swapStep: Partial<{
      sqrtRatioNextX64: JSBI
      amountIn: JSBI
      amountOut: JSBI
      feeAmount: JSBI
    }> = {}

    const zeroForOne = JSBI.greaterThanOrEqual(sqrtRatioCurrentX64, sqrtRatioTargetX64)
    const exactIn = JSBI.greaterThanOrEqual(amountRemaining, ZERO)

    if (exactIn) {
      const amountRemainingLessFee = FullMath.mulDivFloor(
        amountRemaining,
        JSBI.subtract(MAX_FEE, JSBI.BigInt(feePips)),
        MAX_FEE
      )
      swapStep.amountIn = zeroForOne
        ? SqrtPriceMath.getAmount0Delta(sqrtRatioTargetX64, sqrtRatioCurrentX64, liquidity, true)
        : SqrtPriceMath.getAmount1Delta(sqrtRatioCurrentX64, sqrtRatioTargetX64, liquidity, true)
      if (JSBI.greaterThanOrEqual(amountRemainingLessFee, swapStep.amountIn!)) {
        swapStep.sqrtRatioNextX64 = sqrtRatioTargetX64
      } else {
        swapStep.sqrtRatioNextX64 = SqrtPriceMath.getNextSqrtPriceFromInput(
          sqrtRatioCurrentX64,
          liquidity,
          amountRemainingLessFee,
          zeroForOne
        )
      }
      console.log(" computeSwapStep swapStep.amountIn: ", swapStep.amountIn.toString())
    } else {
      swapStep.amountOut = zeroForOne
        ? SqrtPriceMath.getAmount1Delta(sqrtRatioTargetX64, sqrtRatioCurrentX64, liquidity, false)
        : SqrtPriceMath.getAmount0Delta(sqrtRatioCurrentX64, sqrtRatioTargetX64, liquidity, false)
      if (JSBI.greaterThanOrEqual(JSBI.multiply(amountRemaining, NEGATIVE_ONE), swapStep.amountOut)) {
        swapStep.sqrtRatioNextX64 = sqrtRatioTargetX64
      } else {
        swapStep.sqrtRatioNextX64 = SqrtPriceMath.getNextSqrtPriceFromOutput(
          sqrtRatioCurrentX64,
          liquidity,
          JSBI.multiply(amountRemaining, NEGATIVE_ONE),
          zeroForOne
        )
      }
    }

    const max = JSBI.equal(sqrtRatioTargetX64, swapStep.sqrtRatioNextX64)

    if (zeroForOne) {
      swapStep.amountIn =
        max && exactIn
          ? swapStep.amountIn
          : SqrtPriceMath.getAmount0Delta(swapStep.sqrtRatioNextX64, sqrtRatioCurrentX64, liquidity, true)
      swapStep.amountOut =
        max && !exactIn
          ? swapStep.amountOut
          : SqrtPriceMath.getAmount1Delta(swapStep.sqrtRatioNextX64, sqrtRatioCurrentX64, liquidity, false)
    } else {
      swapStep.amountIn =
        max && exactIn
          ? swapStep.amountIn
          : SqrtPriceMath.getAmount1Delta(sqrtRatioCurrentX64, swapStep.sqrtRatioNextX64, liquidity, true)
      swapStep.amountOut =
        max && !exactIn
          ? swapStep.amountOut
          : SqrtPriceMath.getAmount0Delta(sqrtRatioCurrentX64, swapStep.sqrtRatioNextX64, liquidity, false)
    }

    if (!exactIn && JSBI.greaterThan(swapStep.amountOut!, JSBI.multiply(amountRemaining, NEGATIVE_ONE))) {
      swapStep.amountOut = JSBI.multiply(amountRemaining, NEGATIVE_ONE)
    }

    if (exactIn && JSBI.notEqual(swapStep.sqrtRatioNextX64, sqrtRatioTargetX64)) {
      // we didn't reach the target, so take the remainder of the maximum input as fee
      swapStep.feeAmount = JSBI.subtract(amountRemaining, swapStep.amountIn!)
    } else {
      swapStep.feeAmount = FullMath.mulDivCeil(
        swapStep.amountIn!,
        JSBI.BigInt(feePips),
        JSBI.subtract(MAX_FEE, JSBI.BigInt(feePips))
      )
    }

    return [swapStep.sqrtRatioNextX64!, swapStep.amountIn!, swapStep.amountOut!, swapStep.feeAmount!]
  }
}
