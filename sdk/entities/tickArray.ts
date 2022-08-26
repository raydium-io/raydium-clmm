import { BN } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { getTickArrayAddress } from "../utils";
import { TickArrayState, TickState } from "../states";
export const TICK_ARRAY_SIZE = 60;
const TICK_ARRAY_BITMAP_SIZE = 1024;

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
 * @param programId
 * @param poolId
 * @param tickIndex
 * @param tickSpacing
 * @returns
 */
export async function getTickArrayAddressByTick(
  programId: PublicKey,
  poolId: PublicKey,
  tickIndex: number,
  tickSpacing: number
): Promise<PublicKey> {
  const startIndex = getTickArrayStartIndexByTick(tickIndex, tickSpacing);
  let [tickArrayAddress] = await getTickArrayAddress(
    poolId,
    programId,
    startIndex
  );
  return tickArrayAddress;
}

export function getTickOffsetInArray(
  tick_index: number,
  tick_spacing: number
): number {
  if (tick_index % tick_spacing != 0) {
    throw new Error("tick_index % tick_spacing not equal 0");
  }
  let start_tick_index = getTickArrayStartIndexByTick(tick_index, tick_spacing);
  let offset_in_array = Math.floor(
    (tick_index - start_tick_index) / tick_spacing
  );
  if (offset_in_array < 0 || offset_in_array >= TICK_ARRAY_SIZE) {
    throw new Error("tick offset in array overflow");
  }
  return offset_in_array;
}

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
  let startIndex: number = tickIndex / (TICK_ARRAY_SIZE * tickSpacing);
  if (tickIndex < 0 && tickIndex % (TICK_ARRAY_SIZE * tickSpacing) != 0) {
    startIndex = Math.ceil(startIndex) - 1;
  } else {
    startIndex = Math.floor(startIndex);
  }
  return startIndex * (tickSpacing * TICK_ARRAY_SIZE);
}

export function getTickArrayOffsetInBitmapByTick(
  tick: number,
  tickSpacing: number
): number {
  let multiplier = tickSpacing * TICK_ARRAY_SIZE;
  let compressed = Math.floor(tick / multiplier) + 512;
  return Math.abs(compressed);
}

/**
 *
 * @param bitmap
 * @param tick
 * @param tickSpacing
 * @returns if the special bit is initialized and tick array start index
 */
export function checkTickArrayIsInitialized(
  bitmap: BN,
  tick: number,
  tickSpacing: number
): [boolean, number] {
  let multiplier = tickSpacing * TICK_ARRAY_SIZE;
  let compressed = Math.floor(tick / multiplier) + 512;
  let bit_pos = Math.abs(compressed);
  return [bitmap.testn(bit_pos), (bit_pos - 512) * multiplier];
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
  if (tickArrayStartIndex % (tickSpacing * TICK_ARRAY_SIZE) != 0) {
    throw new Error("Invild tickArrayStartIndex");
  }
  let tickArrayOffset =
    Math.floor(tickArrayStartIndex / (tickSpacing * TICK_ARRAY_SIZE)) + 512;
  let result: number[] = [];

  // find right of currenct offset
  result.push(
    ...searchLowBitFromStart(
      tickArrayBitmap,
      tickArrayOffset - 1,
      0,
      expectedCount,
      tickSpacing
    )
  );
  // find left of current offset
  result.push(
    ...searchHightBitFromStart(
      tickArrayBitmap,
      tickArrayOffset,
      TICK_ARRAY_BITMAP_SIZE,
      expectedCount,
      tickSpacing
    )
  );

  return result;
}

export function getAllInitializedTickArrayStartIndex(
  tickArrayBitmap: BN,
  tickSpacing: number
): number[] {
  // find from offset 0 to 1024
  return searchHightBitFromStart(
    tickArrayBitmap,
    0,
    TICK_ARRAY_BITMAP_SIZE,
    TICK_ARRAY_BITMAP_SIZE,
    tickSpacing
  );
}

export async function getAllInitializedTickArrayInfo(
  programId: PublicKey,
  poolId: PublicKey,
  tickArrayBitmap: BN,
  tickSpacing: number
): Promise<
  {
    tickArrayStartIndex: number;
    tickArrayAddress: PublicKey;
  }[]
> {
  let result: {
    tickArrayStartIndex: number;
    tickArrayAddress: PublicKey;
  }[] = [];
  let allInitializedTickArrayIndex: number[] =
    getAllInitializedTickArrayStartIndex(tickArrayBitmap, tickSpacing);
  for (const startIndex of allInitializedTickArrayIndex) {
    const [address] = await getTickArrayAddress(poolId, programId, startIndex);
    result.push({
      tickArrayStartIndex: startIndex,
      tickArrayAddress: address,
    });
  }
  return result;
}

export function getAllInitializedTickInTickArray(
  tickArray: TickArrayState
): TickState[] {
  let allInitializedTick: TickState[] = [];
  for (let i = 0; i < tickArray.ticks.length; i++) {
    if (tickArray.ticks[i].liquidityGross.gtn(0)) {
      allInitializedTick.push(tickArray.ticks[i]);
    }
  }
  return allInitializedTick;
}

/**
 * search for price decrease direction
 * @param tickArrayBitmap
 * @param start
 * @param end
 * @param expectedCount
 * @param tickSpacing
 * @returns
 */
export function searchLowBitFromStart(
  tickArrayBitmap: BN,
  start: number,
  end: number,
  expectedCount: number,
  tickSpacing: number
): number[] {
  let fetchNum: number = 0;
  let result: number[] = [];
  for (let i = start; i >= end; i--) {
    if (tickArrayBitmap.shrn(i).and(new BN(1)).eqn(1)) {
      let nextStartIndex = (i - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  return result;
}

export function searchHightBitFromStart(
  tickArrayBitmap: BN,
  start: number,
  end: number,
  expectedCount: number,
  tickSpacing: number
): number[] {
  let fetchNum: number = 0;
  let result: number[] = [];
  for (let i = start; i < end; i++) {
    if (tickArrayBitmap.shrn(i).and(new BN(1)).eqn(1)) {
      let nextStartIndex = (i - 512) * (tickSpacing * TICK_ARRAY_SIZE);
      result.push(nextStartIndex);
      fetchNum++;
    }
    if (fetchNum >= expectedCount) {
      break;
    }
  }
  return result;
}
