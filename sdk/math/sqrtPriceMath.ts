import {
  ONE,
  ZERO,
  U64Resolution,
  MaxUint128,
  MIN_TICK,
  MAX_TICK,
  MIN_SQRT_PRICE_X64,
  MAX_SQRT_PRICE_X64,
} from "./constants";
import { Math } from "./math";
import { BN } from "@project-serum/anchor";

import Decimal from "decimal.js";

const BIT_PRECISION = 14;
const LOG_B_2_X32 = "59543866431248";
const LOG_B_P_ERR_MARGIN_LOWER_X64 = "184467440737095516";
const LOG_B_P_ERR_MARGIN_UPPER_X64 = "15793534762490258745";

function mulRightShift(val: BN, mulBy: BN): BN {
  return signedRightShift(val.mul(mulBy), 64, 256);
}

function signedLeftShift(n0: BN, shiftBy: number, bitWidth: number) {
  let twosN0 = n0.toTwos(bitWidth).shln(shiftBy);
  twosN0.imaskn(bitWidth + 1);
  return twosN0.fromTwos(bitWidth);
}

function signedRightShift(n0: BN, shiftBy: number, bitWidth: number) {
  let twoN0 = n0.toTwos(bitWidth).shrn(shiftBy);
  twoN0.imaskn(bitWidth - shiftBy + 1);
  return twoN0.fromTwos(bitWidth - shiftBy);
}

export abstract class SqrtPriceMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static sqrtPriceX64ToPrice(sqrtPriceX64: BN): Decimal {
    return Math.x64ToDecimal(sqrtPriceX64).pow(2);
  }
  
  public static priceToSqrtPriceX64(price: Decimal): BN {
    return Math.decimalToX64(price.sqrt());
  }
  
  /**
   *
   * @param sqrtPriceX64
   * @param liquidity
   * @param amountIn
   * @param zeroForOne
   * @returns
   */
  public static getNextSqrtPriceX64FromInput(
    sqrtPriceX64: BN,
    liquidity: BN,
    amountIn: BN,
    zeroForOne: boolean
  ): BN {
    if (!sqrtPriceX64.gt(ZERO)) {
      throw new Error("sqrtPriceX64 must greater than 0");
    }
    if (!liquidity.gt(ZERO)) {
      throw new Error("liquidity must greater than 0");
    }

    return zeroForOne
      ? this.getNextSqrtPriceFromToken0AmountRoundingUp(
          sqrtPriceX64,
          liquidity,
          amountIn,
          true
        )
      : this.getNextSqrtPriceFromToken1AmountRoundingDown(
          sqrtPriceX64,
          liquidity,
          amountIn,
          true
        );
  }

  /**
   *
   * @param sqrtPriceX64
   * @param liquidity
   * @param amountOut
   * @param zeroForOne
   * @returns
   */
  public static getNextSqrtPriceX64FromOutput(
    sqrtPriceX64: BN,
    liquidity: BN,
    amountOut: BN,
    zeroForOne: boolean
  ): BN {
    if (!sqrtPriceX64.gt(ZERO)) {
      throw new Error("sqrtPriceX64 must greater than 0");
    }
    if (!liquidity.gt(ZERO)) {
      throw new Error("liquidity must greater than 0");
    }

    return zeroForOne
      ? this.getNextSqrtPriceFromToken1AmountRoundingDown(
          sqrtPriceX64,
          liquidity,
          amountOut,
          false
        )
      : this.getNextSqrtPriceFromToken0AmountRoundingUp(
          sqrtPriceX64,
          liquidity,
          amountOut,
          false
        );
  }

  /**
   * `√P' = √P * L / (L + Δx * √P)` -> `√P' = L / (L/√P + Δx)
   * @param sqrtPriceX64
   * @param liquidity
   * @param amount
   * @param add Whether to add or remove the amount of token_0
   * @returns
   */
  private static getNextSqrtPriceFromToken0AmountRoundingUp(
    sqrtPriceX64: BN,
    liquidity: BN,
    amount: BN,
    add: boolean
  ): BN {
    if (amount.eq(ZERO)) return sqrtPriceX64;
    let liquidityLeftShift = liquidity.shln(U64Resolution);

    if (add) {
      const numerator1 = liquidityLeftShift;
      const denominator = liquidityLeftShift.add(amount.mul(sqrtPriceX64));
      if (denominator.gte(numerator1)) {
        return Math.mulDivCeil(numerator1, sqrtPriceX64, denominator);
      }
      return Math.mulDivRoundingUp(
        numerator1,
        ONE,
        numerator1.div(sqrtPriceX64).add(amount)
      );
    } else {
      let product = amount.mul(sqrtPriceX64);
      if (liquidityLeftShift.gt(product)) {
        throw new Error("too small");
      }
      const denominator = liquidityLeftShift.sub(product);
      return Math.mulDivCeil(liquidityLeftShift, sqrtPriceX64, denominator);
    }
  }

  /**
   *  `√P' = √P + Δy / L`
   * @param sqrtPriceX64
   * @param liquidity
   * @param amount
   * @param add
   * @returns
   */
  private static getNextSqrtPriceFromToken1AmountRoundingDown(
    sqrtPriceX64: BN,
    liquidity: BN,
    amount: BN,
    add: boolean
  ): BN {
    const deltaY = amount.shln(U64Resolution);
    if (add) {
      return sqrtPriceX64.add(deltaY.div(liquidity));
    } else {
      const quotient = Math.mulDivRoundingUp(deltaY, ONE, liquidity);
      if (!sqrtPriceX64.gt(quotient)) {
        throw new Error("too small");
      }
      return sqrtPriceX64.sub(quotient);
    }
  }

  /**
   * 
   * @param tick 
   * @returns 
   */
  public static getSqrtPriceX64FromTick(tick: number): BN {
    if (!Number.isInteger(tick)) {
      throw new Error("tick must be integer");
    }
    if (tick < MIN_TICK || tick > MAX_TICK) {
      throw new Error("tick must be in MIN_TICK and MAX_TICK");
    }
    const tickAbs: number = tick < 0 ? tick * -1 : tick;

    let ratio: BN =
      (tickAbs & 0x1) != 0
        ? new BN("18445821805675395072")
        : new BN("18446744073709551616");
    if ((tickAbs & 0x2) != 0)
      ratio = mulRightShift(ratio, new BN("18444899583751176192"));
    if ((tickAbs & 0x4) != 0)
      ratio = mulRightShift(ratio, new BN("18443055278223355904"));
    if ((tickAbs & 0x8) != 0)
      ratio = mulRightShift(ratio, new BN("18439367220385607680"));
    if ((tickAbs & 0x10) != 0)
      ratio = mulRightShift(ratio, new BN("18431993317065453568"));
    if ((tickAbs & 0x20) != 0)
      ratio = mulRightShift(ratio, new BN("18417254355718170624"));
    if ((tickAbs & 0x40) != 0)
      ratio = mulRightShift(ratio, new BN("18387811781193609216"));
    if ((tickAbs & 0x80) != 0)
      ratio = mulRightShift(ratio, new BN("18329067761203558400"));
    if ((tickAbs & 0x100) != 0)
      ratio = mulRightShift(ratio, new BN("18212142134806163456"));
    if ((tickAbs & 0x200) != 0)
      ratio = mulRightShift(ratio, new BN("17980523815641700352"));
    if ((tickAbs & 0x400) != 0)
      ratio = mulRightShift(ratio, new BN("17526086738831433728"));
    if ((tickAbs & 0x800) != 0)
      ratio = mulRightShift(ratio, new BN("16651378430235570176"));
    if ((tickAbs & 0x1000) != 0)
      ratio = mulRightShift(ratio, new BN("15030750278694412288"));
    if ((tickAbs & 0x2000) != 0)
      ratio = mulRightShift(ratio, new BN("12247334978884435968"));
    if ((tickAbs & 0x4000) != 0)
      ratio = mulRightShift(ratio, new BN("8131365268886854656"));
    if ((tickAbs & 0x8000) != 0)
      ratio = mulRightShift(ratio, new BN("3584323654725218816"));
    if ((tickAbs & 0x10000) != 0)
      ratio = mulRightShift(ratio, new BN("696457651848324352"));
    if ((tickAbs & 0x20000) != 0)
      ratio = mulRightShift(ratio, new BN("26294789957507116"));
    if ((tickAbs & 0x40000) != 0)
      ratio = mulRightShift(ratio, new BN("37481735321082"));

    if (tick > 0) ratio = MaxUint128.div(ratio);
    return ratio;
  }

  /**
   * 
   * @param price 
   * @returns 
   */
  public static getTickFromPrice(price: Decimal): number {
    return SqrtPriceMath.getTickFromSqrtPriceX64( SqrtPriceMath.priceToSqrtPriceX64(price))
  }


  /**
   *
   * @param sqrtPriceX64
   * @returns
   */
  public static getTickFromSqrtPriceX64(sqrtPriceX64: BN): number {
    if (
      sqrtPriceX64.gt(MAX_SQRT_PRICE_X64) ||
      sqrtPriceX64.lt(MIN_SQRT_PRICE_X64)
    ) {
      throw new Error(
        "Provided sqrtPrice is not within the supported sqrtPrice range."
      );
    }

    const msb = sqrtPriceX64.bitLength() - 1;
    const adjustedMsb = new BN(msb - 64);
    const log2pIntegerX32 = signedLeftShift(adjustedMsb, 32, 128);

    let bit = new BN("8000000000000000", "hex");
    let precision = 0;
    let log2pFractionX64 = new BN(0);

    let r =
      msb >= 64 ? sqrtPriceX64.shrn(msb - 63) : sqrtPriceX64.shln(63 - msb);

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

    const tickLow = signedRightShift(
      logbpX64.sub(new BN(LOG_B_P_ERR_MARGIN_LOWER_X64)),
      64,
      128
    ).toNumber();
    const tickHigh = signedRightShift(
      logbpX64.add(new BN(LOG_B_P_ERR_MARGIN_UPPER_X64)),
      64,
      128
    ).toNumber();

    if (tickLow == tickHigh) {
      return tickLow;
    } else {
      const derivedTickHighSqrtPriceX64 = new BN(
        SqrtPriceMath.getSqrtPriceX64FromTick(tickHigh).toString()
      );
      if (derivedTickHighSqrtPriceX64.lte(sqrtPriceX64)) {
        return tickHigh;
      } else {
        return tickLow;
      }
    }
  }
}
