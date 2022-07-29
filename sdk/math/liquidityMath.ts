import { ONE, ZERO, MaxU64, U64Resolution, Q64 } from "./constants";
import { Math} from "./math";
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
   public static getToken0AmountForLiquidity(
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
      ? Math.mulDivRoundingUp(
          Math.mulDivCeil(numerator1, numerator2, sqrtPriceBX64),
          ONE,
          sqrtPriceAX64
        )
      : Math.mulDivFloor(numerator1, numerator2, sqrtPriceBX64).div(
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
  public static getToken1AmountForLiquidity(
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
      ? Math.mulDivCeil(liquidity, sqrtPriceBX64.sub(sqrtPriceAX64), Q64)
      : Math.mulDivFloor(liquidity, sqrtPriceBX64.sub(sqrtPriceAX64), Q64);
  }

  /**
   * Calculates ΔL = Δx (√P_upper x √P_lower)/(√P_upper - √P_lower)
   * @param sqrtPriceAX64
   * @param sqrtPriceBX64
   * @param amount0
   * @returns
   */
  public static maxLiquidityFromToken0Amount(
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
      return Math.mulDivRoundingUp(result, ONE, MaxU64);
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
  public static maxLiquidityFromToken1Amount(
    sqrtPriceAX64: BN,
    sqrtPriceBX64: BN,
    amount1: BN
  ): BN {
    if (sqrtPriceAX64.gt(sqrtPriceBX64)) {
      [sqrtPriceAX64, sqrtPriceBX64] = [sqrtPriceBX64, sqrtPriceAX64];
    }
    return Math.mulDivFloor(amount1, MaxU64, sqrtPriceBX64.sub(sqrtPriceAX64));
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
  public static maxLiquidityFromTokenAmounts(
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
      return LiquidityMath.maxLiquidityFromToken0Amount(
        sqrtPriceAX64,
        sqrtPriceBX64,
        amount0,
        false
      );
    } else if (sqrtPriceCurrentX64.lt(sqrtPriceBX64)) {
      const liquidity0 = LiquidityMath.maxLiquidityFromToken0Amount(
        sqrtPriceCurrentX64,
        sqrtPriceBX64,
        amount0,
        false
      );
      const liquidity1 = LiquidityMath.maxLiquidityFromToken1Amount(
        sqrtPriceAX64,
        sqrtPriceCurrentX64,
        amount1
      );
      return liquidity0.lt(liquidity1) ? liquidity0 : liquidity1;
    } else {
      return LiquidityMath.maxLiquidityFromToken1Amount(
        sqrtPriceAX64,
        sqrtPriceBX64,
        amount1
      );
    }
  }
}
