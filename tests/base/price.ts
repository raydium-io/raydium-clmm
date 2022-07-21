import { BN } from "@project-serum/anchor";
import { BigNumberish, Currency, Logger } from "@raydium-io/raydium-sdk";
import Decimal from "decimal.js";
import {Math} from "../math";

const logger = Logger.from("base/price");

export class SqrtPrice {
  private sqrtPrice: BN;

  // public constructor(price: BN | string) {
  //   if (price instanceof BN) {
  //     this.sqrt_price_x64 = price;
  //   } else if (typeof price === "string") {
  //     this.sqrt_price_x64 = new BN(price);
  //   } else {
  //     logger.throwArgumentError("invalid type", "BigNumberish", price);
  //   }
  // }

  public static fromX64(price: BN | string):SqrtPrice{
    const p = new SqrtPrice()
    if (price instanceof BN) {
      p.sqrtPrice = price;
    } else if (typeof price === "string") {
      p.sqrtPrice = new BN(price);
    } else {
      logger.throwArgumentError("invalid type", "BigNumberish", price);
    }
    return p
  }

  public static fromDecimal(price: Decimal | string):SqrtPrice{
    const p = new SqrtPrice()
    if (price instanceof Decimal) {
      p.sqrtPrice = Math.decimalToX64(price);
    } else if (typeof price === "string") {
      p.sqrtPrice = Math.decimalToX64(new Decimal(price));
    } else {
      logger.throwArgumentError("invalid type", "BigNumberish", price);
    }
    return p
  }

  public to_price(precision: number): string {
    return Math.x64ToDecimal(this.sqrtPrice).toPrecision(precision)
  }
}
