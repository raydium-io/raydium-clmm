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
    .add(bns[7].shln(448))
    .add(bns[8].shln(512))
    .add(bns[9].shln(576))
    .add(bns[10].shln(640))
    .add(bns[11].shln(704))
    .add(bns[12].shln(768))
    .add(bns[13].shln(832))
    .add(bns[14].shln(896))
    .add(bns[15].shln(960));
}

/**
 *
 * @param tickArrayBitmap
 * @param tickSpacing
 * @param tickArrayStartIndex
 * @param expectedCount
 * @returns
 */
export function getInitializedTickArrayInRange(
  tickArrayBitmap: BN,
  tickSpacing: number,
  tickArrayStartIndex: number,
  expectedCount: number
): number[] {
  let tickArrayOffset =
    Math.floor(tickArrayStartIndex / (tickSpacing * TICK_ARRAY_SIZE)) + 512;
  let result: number[] = [];
  let isPositive = true;
  if (tickArrayStartIndex < 0) {
    isPositive = false;
  }
  // find left of current offset
  result.push(
    ...searchLeftFromStart(
      tickArrayBitmap,
      tickArrayOffset,
      1024,
      expectedCount,
      tickSpacing,
      isPositive
    )
  );
  // find right of currenct offset
  result.push(
    ...searchRightFromStart(
      tickArrayBitmap,
      tickArrayOffset - 1,
      0,
      expectedCount,
      tickSpacing,
      isPositive
    )
  );
  return result;
}

/**
 * search for price decrease direction
 * @param tickArrayBitmap
 * @param start
 * @param end
 * @param expectedCount
 * @param tickSpacing
 * @param isPositive
 * @returns
 */
function searchRightFromStart(
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
        nextStartIndex = (i - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      } else {
        nextStartIndex = (-i - 1 - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      }
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  console.log("searchRightFromStart:", result);
  return result;
}

function searchLeftFromStart(
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
        nextStartIndex = (i - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      } else {
        nextStartIndex = (-i - 1 - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      }
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  console.log("searchLeftFromStart:", result);
  return result;
}
