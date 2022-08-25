import { ONE, ZERO, MaxU64, U64Resolution, Q64 } from "./constants";
import { MathUtil } from "./mathUtil";
// import { } from "../math/constants";
import { BN } from "@project-serum/anchor";

export abstract class LiquidityMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  /**
   *
   * @param x
   * @param y can be negative
   * @returns
   */
  public static addDelta(x: BN, y: BN): BN {
    return x.add(y);
  }

  /**
   * Calculates Δx = ΔL (√P_upper - √P_lower) / (√P_upper x √P_lower)
   * @param sqrtPriceAX64
   * @param sqrtPriceBX64
   * @param liquidity
   * @param roundUp
   * @returns
   */
  public static getToken0AmountFromLiquidity(
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    liquidity: BN,
    roundUp: boolean
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }

    if (!sqrtPriceAX64.gt(ZERO)) {
      throw new Error("sqrtPriceAX64 must greater than 0");
    }

    const numerator1 = liquidity.ushln(U64Resolution);
    const numerator2 = sqrtPriceBX64.sub(sqrtPriceAX64);

    return roundUp
      ? MathUtil.mulDivRoundingUp(
          MathUtil.mulDivCeil(numerator1, numerator2, sqrtPriceBX64),
          ONE,
          sqrtPriceAX64
        )
      : MathUtil.mulDivFloor(numerator1, numerator2, sqrtPriceBX64).div(
          sqrtPriceAX64
        );
  }

  /**
   * Calculates Δy = ΔL * (√P_upper - √P_lower)
   * @param sqrtPriceAX64
   * @param sqrtPriceBX64
   * @param liquidity
   * @param roundUp
   * @returns
   */
  public static getToken1AmountFromLiquidity(
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    liquidity: BN,
    roundUp: boolean
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }
    if (!sqrtPriceAX64.gt(ZERO)) {
      throw new Error("sqrtPriceAX64 must greater than 0");
    }

    return roundUp
      ? MathUtil.mulDivCeil(liquidity, sqrtPriceBX64.sub(sqrtPriceAX64), Q64)
      : MathUtil.mulDivFloor(liquidity, sqrtPriceBX64.sub(sqrtPriceAX64), Q64);
  }

  /**
   * Calculates ΔL = Δx (√P_upper x √P_lower)/(√P_upper - √P_lower)
   * @param sqrtPriceAX64
   * @param sqrtPriceBX64
   * @param amount0
   * @returns
   */
  public static getLiquidityFromToken0Amount(
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    amount0: BN,
    roundUp: boolean
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }

    const numerator = amount0.mul(sqrtPriceAX64).mul(sqrtPriceBX64);
    const denominator = sqrtPriceBX64.sub(sqrtPriceAX64);
    let result = numerator.div(denominator);

    if (roundUp) {
      return MathUtil.mulDivRoundingUp(result, ONE, MaxU64);
    } else {
      return result.shrn(U64Resolution);
    }
  }

  /**
   * Computes the maximum amount of liquidity received for a given amount of token1, ΔL = Δy / (√P_upper - √P_lower)
   * @param sqrtPriceAX64 The price at the lower tick boundary
   * @param sqrtPriceBX64 The price at the upper tick boundary
   * @param amount1 The token1 amount
   * @returns liquidity for amount1
   */
  public static getLiquidityFromToken1Amount(
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    amount1: BN
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }
    return MathUtil.mulDivFloor(amount1, MaxU64, sqrtPriceBX64.sub(sqrtPriceAX64));
  }

  /**
   * Computes the maximum amount of liquidity received for a given amount of token0, token1,
   * and the prices at the tick boundaries.
   * @param sqrtPriceCurrentX64 the current price
   * @param sqrtPriceAX64 price at lower boundary
   * @param sqrtPriceBX64 price at upper boundary
   * @param amount0 token0 amount
   * @param amount1 token1 amount
   * not what core can theoretically support
   */
  public static getLiquidityFromTokenAmounts(
    sqrtPriceCurrentX64: BN,
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    amount0: BN,
    amount1: BN
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }

    if (sqrtPriceCurrentX64.lte(sqrtPriceAX64)) {
      return LiquidityMath.getLiquidityFromToken0Amount(
        sqrtPriceAX64,
        sqrtPriceBX64,
        amount0,
        false
      );
    } else if (sqrtPriceCurrentX64.lt(sqrtPriceBX64)) {
      const liquidity0 = LiquidityMath.getLiquidityFromToken0Amount(
        sqrtPriceCurrentX64,
        sqrtPriceBX64,
        amount0,
        false
      );
      const liquidity1 = LiquidityMath.getLiquidityFromToken1Amount(
        sqrtPriceAX64,
        sqrtPriceCurrentX64,
        amount1
      );
      return liquidity0.lt(liquidity1) ? liquidity0 : liquidity1;
    } else {
      return LiquidityMath.getLiquidityFromToken1Amount(
        sqrtPriceAX64,
        sqrtPriceBX64,
        amount1
      );
    }
  }

  public static getAmountsFromLiquidity(
    sqrtPriceCurrentX64: BN,
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    liquidity: BN,
    roundUp: boolean
  ): [BN, BN] {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }

    if (sqrtPriceCurrentX64.lte(sqrtPriceAX64)) {
      return [
        LiquidityMath.getToken0AmountFromLiquidity(
          sqrtPriceAX64,
          sqrtPriceBX64,
          liquidity,
          roundUp
        ),
        new BN(0),
      ];
    } else if (sqrtPriceCurrentX64.lt(sqrtPriceBX64)) {
      const amount0 = LiquidityMath.getToken0AmountFromLiquidity(
        sqrtPriceCurrentX64,
        sqrtPriceBX64,
        liquidity,
        roundUp
      );
      const amount1 = LiquidityMath.getToken1AmountFromLiquidity(
        sqrtPriceAX64,
        sqrtPriceCurrentX64,
        liquidity,
        roundUp
      );
      return [amount0, amount1];
    } else {
      return [
        new BN(0),
        LiquidityMath.getToken1AmountFromLiquidity(
          sqrtPriceAX64,
          sqrtPriceBX64,
          liquidity,
          roundUp
        ),
      ];
    }
  }

  public static getAmountsFromLiquidityWithSlippage(
    sqrtPriceCurrentX64: BN,
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    liquidity: BN,
    amountMax: boolean,
    roundUp: boolean,
    amountSlippage?: number
  ): [BN, BN] {
    const [token0Amount, token1Amount] = LiquidityMath.getAmountsFromLiquidity(
      sqrtPriceCurrentX64,
      sqrtPriceAX64,
      sqrtPriceBX64,
      liquidity,
      roundUp
    );
    let coefficient = 1 + amountSlippage;
    if (!amountMax) {
      coefficient = 1 - amountSlippage;
    }
    let amount0Slippage: BN = token0Amount;
    let amount1Slippage: BN = token1Amount;
    if (!amountMax) {
      amount0Slippage = new BN(0);
      amount1Slippage = new BN(0);
    }
    if (amountSlippage !== undefined) {
      amount0Slippage = token0Amount.muln(coefficient);
      amount1Slippage = token1Amount.muln(coefficient);
    }
    return [amount0Slippage, amount1Slippage];
  }
}
