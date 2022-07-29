import { BN } from "@project-serum/anchor";
import { Fee, FEE_RATE_DENOMINATOR } from "../entities";
import { NEGATIVE_ONE, ZERO } from "./constants";
import { LiquidityMath } from "./liquidityMath";
import { Math } from "./math";
import { SqrtPriceMath } from "./sqrtPriceMath";
import { CacheDataProvider } from "../entities";
import { AccountMeta } from "@solana/web3.js";

import {
  ONE,
  MIN_SQRT_PRICE_X64,
  MAX_SQRT_PRICE_X64,
  MIN_TICK,
  MAX_TICK,
} from "../math";

type SwapStep = {
  sqrtPriceX64Next: BN;
  amountIn: BN;
  amountOut: BN;
  feeAmount: BN;
};

export interface StepComputations {
  sqrtPriceStartX64: BN;
  tickNext: number;
  initialized: boolean;
  sqrtPriceNextX64: BN;
  amountIn: BN;
  amountOut: BN;
  feeAmount: BN;
}

export abstract class SwapMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  /**
   *
   * @param zeroForOne
   * @param amountSpecified  can be negative,
   * @param sqrtPriceLimitX64
   * @returns
   */
  public static async swapCompute(
    cacheDataProvider: CacheDataProvider,
    zeroForOne: boolean,
    fee: number,
    liquidity: BN,
    currentTick: number,
    tickSpacing: number,
    currentSqrtPriceX64: BN,
    amountSpecified: BN,
    sqrtPriceLimitX64?: BN
  ): Promise<{
    amountCalculated: BN;
    sqrtPriceX64: BN;
    liquidity: BN;
    tickCurrent: number;
    accounts: AccountMeta[];
  }> {
    if (amountSpecified.eq(ZERO)) {
      throw new Error("amountSpecified must not be 0");
    }

    if (!sqrtPriceLimitX64)
      sqrtPriceLimitX64 = zeroForOne
        ? MIN_SQRT_PRICE_X64.add(ONE)
        : MAX_SQRT_PRICE_X64.sub(ONE);

    if (zeroForOne) {
      if (sqrtPriceLimitX64.lt(MIN_SQRT_PRICE_X64)) {
        throw new Error("sqrtPriceX64 must greater than MIN_SQRT_PRICE_X64");
      }

      if (sqrtPriceLimitX64.gt(currentSqrtPriceX64)) {
        throw new Error("sqrtPriceX64 must smaller than current");
      }
    } else {
      if (sqrtPriceLimitX64.gt(MAX_SQRT_PRICE_X64)) {
        throw new Error("sqrtPriceX64 must smaller than MAX_SQRT_PRICE_X64");
      }

      if (sqrtPriceLimitX64.lte(currentSqrtPriceX64)) {
        throw new Error("sqrtPriceX64 must greater than current");
      }
    }
    const baseInput = amountSpecified.gt(ZERO);

    const state = {
      amountSpecifiedRemaining: amountSpecified,
      amountCalculated: ZERO,
      sqrtPriceX64: currentSqrtPriceX64,
      tick: currentTick,
      accounts: [] as AccountMeta[],
      liquidity: liquidity,
    };

    let lastSavedTickArrayStartIndex: number | undefined;

    let loopCount = 0;
    // loop across ticks until input liquidity is consumed, or the limit price is reached
    while (
      !state.amountSpecifiedRemaining.eq(ZERO) &&
      state.sqrtPriceX64 != sqrtPriceLimitX64 &&
      state.tick < MAX_TICK &&
      state.tick > MIN_TICK
    ) {
      if (loopCount > 10) {
        throw Error("account limit");
      }

      let step: Partial<StepComputations> = {};
      step.sqrtPriceStartX64 = state.sqrtPriceX64;

      // save the bitmap, and the tick account if it is initialized
      const [nextInitTick, tickArrayAddress, tickAarrayStartIndex] =
        await cacheDataProvider.nextInitializedTick(
          state.tick,
          tickSpacing,
          zeroForOne
        );
      step.tickNext = nextInitTick.tick;
      step.initialized = nextInitTick.liquidityGross.gtn(0);

      if (lastSavedTickArrayStartIndex !== tickAarrayStartIndex) {
        state.accounts.push({
          pubkey: tickArrayAddress,
          isWritable: true,
          isSigner: false,
        });
        lastSavedTickArrayStartIndex = tickAarrayStartIndex;
      }

      if (step.tickNext < MIN_TICK) {
        step.tickNext = MIN_TICK;
      } else if (step.tickNext > MAX_TICK) {
        step.tickNext = MAX_TICK;
      }

      step.sqrtPriceNextX64 = SqrtPriceMath.getSqrtPriceX64FromTick(
        step.tickNext
      );

      let targetPrice: BN;
      let ss: boolean;
      if (
        (zeroForOne && step.sqrtPriceNextX64 < sqrtPriceLimitX64) ||
        (!zeroForOne && step.sqrtPriceNextX64 > sqrtPriceLimitX64)
      ) {
        targetPrice = sqrtPriceLimitX64;
        ss = true;
      } else {
        targetPrice = step.sqrtPriceNextX64;
      }

      [state.sqrtPriceX64, step.amountIn, step.amountOut, step.feeAmount] =
        SwapMath.swapStepCompute(
          state.sqrtPriceX64,
          targetPrice,
          state.liquidity,
          state.amountSpecifiedRemaining,
          fee
        );

      if (baseInput) {
        // subtract the input amount. The loop exits if remaining amount becomes 0
        state.amountSpecifiedRemaining = state.amountSpecifiedRemaining.sub(
          step.amountIn.add(step.feeAmount)
        );
        state.amountCalculated = state.amountCalculated.sub(step.amountOut);
      } else {
        state.amountSpecifiedRemaining = state.amountSpecifiedRemaining.add(
          step.amountOut
        );
        state.amountCalculated = state.amountCalculated.add(
          step.amountIn.add(step.feeAmount)
        );
      }

      if (state.sqrtPriceX64.eq(step.sqrtPriceNextX64)) {
        // if the tick is initialized, run the tick transition
        if (step.initialized) {
          let liquidityNet = nextInitTick.liquidityNet;
          if (zeroForOne) liquidityNet = liquidityNet.mul(NEGATIVE_ONE);

          state.liquidity = LiquidityMath.addDelta(
            state.liquidity,
            liquidityNet
          );
        }
        state.tick = zeroForOne ? step.tickNext - 1 : step.tickNext;
      } else if (state.sqrtPriceX64 != step.sqrtPriceStartX64) {
        state.tick = SqrtPriceMath.getTickFromSqrtPriceX64(state.sqrtPriceX64);
      }

      ++loopCount;
    }

    return {
      amountCalculated: state.amountCalculated,
      sqrtPriceX64: state.sqrtPriceX64,
      liquidity: state.liquidity,
      tickCurrent: state.tick,
      accounts: state.accounts,
    };
  }

  private static swapStepCompute(
    sqrtPriceX64Current: BN,
    sqrtPriceX64Target: BN,
    liquidity: BN,
    amountRemaining: BN,
    feeRate: Fee
  ): [BN, BN, BN, BN] {
    let swapStep: Partial<SwapStep> = {};

    const zeroForOne = sqrtPriceX64Current.gte(sqrtPriceX64Target);
    const baseInput = amountRemaining.gte(ZERO);

    if (baseInput) {
      const amountRemainingSubtractFee = Math.mulDivFloor(
        amountRemaining,
        FEE_RATE_DENOMINATOR.sub(new BN(feeRate.toString())),
        FEE_RATE_DENOMINATOR
      );
      swapStep.amountIn = zeroForOne
        ? LiquidityMath.getToken0AmountForLiquidity(
            sqrtPriceX64Target,
            sqrtPriceX64Current,
            liquidity,
            true
          )
        : LiquidityMath.getToken1AmountForLiquidity(
            sqrtPriceX64Current,
            sqrtPriceX64Target,
            liquidity,
            true
          );
      if (amountRemainingSubtractFee.gte(swapStep.amountIn)) {
        swapStep.sqrtPriceX64Next = sqrtPriceX64Target;
      } else {
        swapStep.sqrtPriceX64Next = SqrtPriceMath.getNextSqrtPriceX64FromInput(
          sqrtPriceX64Current,
          liquidity,
          amountRemainingSubtractFee,
          zeroForOne
        );
      }
    } else {
      swapStep.amountOut = zeroForOne
        ? LiquidityMath.getToken1AmountForLiquidity(
            sqrtPriceX64Target,
            sqrtPriceX64Current,
            liquidity,
            false
          )
        : LiquidityMath.getToken0AmountForLiquidity(
            sqrtPriceX64Current,
            sqrtPriceX64Target,
            liquidity,
            false
          );
      if (amountRemaining.mul(NEGATIVE_ONE).gte(swapStep.amountOut)) {
        swapStep.sqrtPriceX64Next = sqrtPriceX64Target;
      } else {
        swapStep.sqrtPriceX64Next = SqrtPriceMath.getNextSqrtPriceX64FromOutput(
          sqrtPriceX64Current,
          liquidity,
          amountRemaining.mul(NEGATIVE_ONE),
          zeroForOne
        );
      }
    }

    const reachTargetPrice = sqrtPriceX64Target.eq(swapStep.sqrtPriceX64Next);

    if (zeroForOne) {
      if (!(reachTargetPrice && baseInput)) {
        swapStep.amountIn = LiquidityMath.getToken0AmountForLiquidity(
          swapStep.sqrtPriceX64Next,
          sqrtPriceX64Current,
          liquidity,
          true
        );
      }

      if (!(reachTargetPrice && !baseInput)) {
        swapStep.amountOut = LiquidityMath.getToken1AmountForLiquidity(
          swapStep.sqrtPriceX64Next,
          sqrtPriceX64Current,
          liquidity,
          false
        );
      }
    } else {
      swapStep.amountIn =
        reachTargetPrice && baseInput
          ? swapStep.amountIn
          : LiquidityMath.getToken1AmountForLiquidity(
              sqrtPriceX64Current,
              swapStep.sqrtPriceX64Next,
              liquidity,
              true
            );
      swapStep.amountOut =
        reachTargetPrice && !baseInput
          ? swapStep.amountOut
          : LiquidityMath.getToken0AmountForLiquidity(
              sqrtPriceX64Current,
              swapStep.sqrtPriceX64Next,
              liquidity,
              false
            );
    }

    if (
      !baseInput &&
      swapStep.amountOut.gt(amountRemaining.mul(NEGATIVE_ONE))
    ) {
      swapStep.amountOut = amountRemaining.mul(NEGATIVE_ONE);
    }
    if (baseInput && !swapStep.sqrtPriceX64Next.eq(sqrtPriceX64Target)) {
      swapStep.feeAmount = amountRemaining.sub(swapStep.amountIn);
    } else {
      swapStep.feeAmount = Math.mulDivCeil(
        swapStep.amountIn,
        new BN(feeRate),
        FEE_RATE_DENOMINATOR.sub(new BN(feeRate))
      );
    }
    return [
      swapStep.sqrtPriceX64Next,
      swapStep.amountIn,
      swapStep.amountOut,
      swapStep.feeAmount,
    ];
  }
}
