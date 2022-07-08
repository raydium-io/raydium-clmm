import JSBI from 'jsbi'
import invariant from 'tiny-invariant'
import { ONE, ZERO } from '../constants'

export abstract class FullMath {
  /**
   * Cannot be constructed.
   */
  private constructor() {}

  public static mulDivRoundingUp(a: JSBI, b: JSBI, denominator: JSBI): JSBI {
    const product = JSBI.multiply(a, b)
    let result = JSBI.divide(product, denominator)
    if (JSBI.notEqual(JSBI.remainder(product, denominator), ZERO)) result = JSBI.add(result, ONE)
    return result
  }

  public static mulDivFloor(a: JSBI, b: JSBI, denominator: JSBI): JSBI {
    invariant(JSBI.notEqual(denominator, ZERO), 'DIVISION_BY_0')
    const product = JSBI.multiply(a, b)
    return JSBI.divide(product, denominator)
  }

  public static mulDivCeil(a: JSBI, b: JSBI, denominator: JSBI): JSBI {
    invariant(JSBI.notEqual(denominator, ZERO), 'DIVISION_BY_0')
    const product = JSBI.multiply(a, b)
    return JSBI.divide(JSBI.add(product, JSBI.subtract(denominator, ONE)), denominator)
  }
}
