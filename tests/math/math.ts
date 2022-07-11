import { BN } from '@project-serum/anchor'
import { ONE, ZERO,Q64 ,U64Resolution} from './constants'
import Decimal from "decimal.js";

export abstract class Math {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static mulDivRoundingUp(a: BN, b: BN, denominator: BN): BN {
    const numerator = a.mul(b)
    let result = numerator.div(denominator)
    if (!numerator.mod(denominator).eq(ZERO)) {
      result = result.add(ONE)
    }
    return result
  }

  public static mulDivFloor(a: BN, b: BN, denominator: BN): BN {
    if (denominator.eq(ZERO)){
      throw new Error("division by 0");
    }
    return a.mul(b).div(denominator)
  }

  public static mulDivCeil(a: BN, b: BN, denominator: BN): BN {
    if (denominator.eq(ZERO)){
      throw new Error("division by 0");
    }
    const numerator = a.mul(b).add(denominator.sub(ONE))
    return numerator.div(denominator)
  }

  public static x64ToDecimal(num: BN): Decimal {
    return new Decimal(num.toString()).div(Decimal.pow(2, 64)).toDecimalPlaces();
  }

  public static decimalToX64(num: Decimal): BN {
    return new BN(num.mul(Decimal.pow(2, 64)).floor().toFixed());
  }
}
