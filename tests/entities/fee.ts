
import { BN } from '@project-serum/anchor'

export const FEE_RATE_DENOMINATOR = new BN(10).pow(new BN(6))

/**
 * The default factory enabled fee amounts, denominated in hundredths of bips.
 * fee_rate_denominator: 1000000
 */
 export enum Fee {
    rate_500 = 500,  //  500 / 10e6 = 0.0005 
    rate_3000 = 3000, // 3000/ 10e6 = 0.003
    rate_10000 = 10000 // 10000 /10e6 = 0.01
  }
  
  /**
   * The default factory tick spacings by fee amount.
   */
  export const TICK_SPACINGS: { [amount in Fee]: number } = {
    [Fee.rate_500]: 10,
    [Fee.rate_3000]: 60,
    [Fee.rate_10000]: 200
  }
  