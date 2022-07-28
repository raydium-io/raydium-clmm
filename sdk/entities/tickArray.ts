import { BN } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
export const TICK_ARRAY_SIZE = 80;

export declare type Tick = {
  tick: number;
  liquidityNet: BN;
  liquidityGross: BN;
  secondsPerLiquidityOutsideX64: BN;
};

export declare type TickArray = {
  address: PublicKey;
  ammPool: PublicKey;
  startTickIndex: number;
  ticks: Tick[];
};

/**
 *
 * @param tickIndex
 * @param tickSpacing
 * @returns
 */
export function getArrayStartIndex(
  tickIndex: number,
  tickSpacing: number
): number {
  let startIndex: number;
  if (tickIndex < 0) {
    startIndex = Math.ceil(tickIndex / (TICK_ARRAY_SIZE * tickSpacing));
    startIndex = startIndex - 1;
  } else {
    startIndex = Math.floor(tickIndex / (TICK_ARRAY_SIZE * tickSpacing));
  }
  return startIndex * (tickSpacing * TICK_ARRAY_SIZE);
}

/**
 *
 * @param lastTickArrayStartIndex
 * @param tickSpacing
 * @param zeroForOne
 * @returns
 */
export function getNextTickArrayStartIndex(
  lastTickArrayStartIndex: number,
  tickSpacing: number,
  zeroForOne: boolean
): number {
  let nextStartIndex: number;
  if (zeroForOne) {
    nextStartIndex = lastTickArrayStartIndex - tickSpacing * TICK_ARRAY_SIZE;
  } else {
    nextStartIndex = lastTickArrayStartIndex + tickSpacing * TICK_ARRAY_SIZE;
  }
  return nextStartIndex;
}
