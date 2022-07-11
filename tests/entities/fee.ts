

/**
 * The default factory enabled fee amounts, denominated in hundredths of bips.
 * fee_rate_denominator: 1000000
 */
 export enum FeeAmount {
    LOW = 500,  // 0.05%
    MEDIUM = 3000, // 0.3%
    HIGH = 10000 // 1.00%
  }
  
  /**
   * The default factory tick spacings by fee amount.
   */
  export const TICK_SPACINGS: { [amount in FeeAmount]: number } = {
    [FeeAmount.LOW]: 10,
    [FeeAmount.MEDIUM]: 60,
    [FeeAmount.HIGH]: 200
  }
  