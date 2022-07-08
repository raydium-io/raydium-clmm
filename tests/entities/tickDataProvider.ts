import { BigintIsh } from '@cykura/sdk-core'
import * as anchor from  '@project-serum/anchor'
import { PublicKey } from '@solana/web3.js'
import JSBI from 'jsbi'

/**
 * Provides information about ticks
 */
export interface TickDataProvider {
  /**
   * Return information corresponding to a specific tick
   * @param tick the tick to load
   */
  getTick(tick: number): {
    address: anchor.web3.PublicKey;
    liquidityNet: JSBI;
  }

  /**
   * Return the PDA corresponding to a specific tick
   * @param tick get PDA for this tick
   */
  getTickAddress(tick: number): Promise<anchor.web3.PublicKey>

  /**
   * Return the next tick that is initialized within a single word
   * @param tick The current tick
   * @param lte Whether the next tick should be lte the current tick
   * @param tickSpacing The tick spacing of the pool
   */
  nextInitializedTickWithinOneWord(tick: number, lte: boolean, tickSpacing: number)
    : [number, boolean, number, number, PublicKey]
}

/**
 * This tick data provider does not know how to fetch any tick data. It throws whenever it is required. Useful if you
 * do not need to load tick data for your use case.
 */
export class NoTickDataProvider implements TickDataProvider {
  getTickAddress(tick: number): Promise<anchor.web3.PublicKey> {
    throw new Error('Method not implemented.')
  }
  private static ERROR_MESSAGE = 'No tick data provider was given'
  getTick(_tick: number): {
    address: anchor.web3.PublicKey;
    liquidityNet: JSBI;
  } {
    throw new Error(NoTickDataProvider.ERROR_MESSAGE)
  }

  nextInitializedTickWithinOneWord(
    _tick: number,
    _lte: boolean,
    _tickSpacing: number
  ): [number, boolean, number, number, PublicKey] {
    throw new Error(NoTickDataProvider.ERROR_MESSAGE)
  }
}

export type PoolVars = {
  token0: PublicKey,
  token1: PublicKey,
  fee: number,
}
