import JSBI from 'jsbi'
import { NEGATIVE_ONE, ZERO } from '../constants'

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
}