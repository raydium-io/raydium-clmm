

/**
 * The default factory enabled fee amounts, denominated in hundredths of bips.
 */
 export enum FeeAmount {
    SUPER_STABLE = 20,
    TURBO_SPL = 80,
    LOW = 500, // deprecated
    MEDIUM = 3000, // new high tier
    HIGH = 10000 // deprecated
  }
  
  /**
   * The default factory tick spacings by fee amount.
   */
  export const TICK_SPACINGS: { [amount in FeeAmount]: number } = {
    [FeeAmount.SUPER_STABLE]: 1,
    [FeeAmount.TURBO_SPL]: 60,
    [FeeAmount.LOW]: 10,
    [FeeAmount.MEDIUM]: 60,
    [FeeAmount.HIGH]: 200
  }
  