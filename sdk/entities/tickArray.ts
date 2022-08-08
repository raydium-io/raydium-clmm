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
 * @returns return start index of  tick array whick contain the given tick
 */
export function getTickArrayStartIndexByTick(
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

export function mergeTickArrayBitmap(bns: BN[]) {
  return bns[0]
    .add(bns[1].shln(64))
    .add(bns[2].shln(128))
    .add(bns[3].shln(192))
    .add(bns[4].shln(256))
    .add(bns[5].shln(320))
    .add(bns[6].shln(384))
    .add(bns[7].shln(448));
}

/**
 *
 * @param tickArrayBitmapPositive
 * @param tickArrayBitmapNegative
 * @param tickSpacing
 * @param tickArrayStartIndex
 * @param expectedCount
 * @returns
 */
export function getInitializedTickArrayInRange(
  tickArrayBitmapPositive: BN,
  tickArrayBitmapNegative: BN,
  tickSpacing: number,
  tickArrayStartIndex: number,
  expectedCount: number
): number[] {
  let tickArrayOffset = Math.floor(
    tickArrayStartIndex / (tickSpacing * TICK_ARRAY_SIZE)
  );
  let fetchNum: number = 0;
  let result: number[] = [];
  let currTickArrayBitmap = tickArrayBitmapPositive;
  let nextTickArrayBitmap = tickArrayBitmapNegative;
  let isPositive = true
  if (tickArrayStartIndex < 0) {
    currTickArrayBitmap = tickArrayBitmapNegative;
    nextTickArrayBitmap = tickArrayBitmapPositive;
    isPositive = false
  }
  // find left of current offset
  result.push(...findLeft(currTickArrayBitmap,tickArrayOffset,0,expectedCount,tickSpacing,isPositive))
  if (isPositive) {
    // if can't find enough, we need to continue searching across the boundary
    if (fetchNum < expectedCount) {
      result.push(...findRight(nextTickArrayBitmap,0,512,expectedCount-fetchNum,tickSpacing,false))
    }
  }

  fetchNum = 0;
  // find right of currenct offset
  result.push(...findRight(currTickArrayBitmap,tickArrayOffset + 1,512,expectedCount,tickSpacing,isPositive))
  if (!isPositive) {
    if (fetchNum < expectedCount) {
      result.push(...findRight(nextTickArrayBitmap,0,512,expectedCount-fetchNum,tickSpacing,true))
    }
  }
  return result;
}

function findLeft(
  tickArrayBitmap: BN,
  start: number,
  end: number,
  expectedCount: number,
  tickSpacing: number,
  isPositive: boolean
): number[] {
  let fetchNum: number = 0;
  let result: number[] = [];
  for (let i = start; i >= end; i--) {
    if (tickArrayBitmap.shrn(i).and(new BN(1)).eqn(1)) {
      let nextStartIndex = 0;
      if (isPositive) {
        nextStartIndex = i * (tickSpacing * TICK_ARRAY_SIZE);
      } else {
        nextStartIndex = (-i - 1) * (tickSpacing * TICK_ARRAY_SIZE);
      }
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  return result
}

function findRight(
  tickArrayBitmap: BN,
  start: number,
  end: number,
  expectedCount: number,
  tickSpacing: number,
  isPositive: boolean
): number[] {
  let fetchNum: number = 0;
  let result: number[] = [];
  for (let i = start; i < end; i++) {
    if (tickArrayBitmap.shrn(i).and(new BN(1)).eqn(1)) {
      let nextStartIndex = 0;
      if (isPositive) {
        nextStartIndex = i * (tickSpacing * TICK_ARRAY_SIZE);
      } else {
        nextStartIndex = (-i - 1) * (tickSpacing * TICK_ARRAY_SIZE);
      }
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  return result
}
