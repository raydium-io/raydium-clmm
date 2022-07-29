import * as anchor from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { BN } from "@project-serum/anchor";

import { MAX_TICK, MIN_TICK } from "../math";
import { CacheDataProvider } from "../entities/cacheProvider";
import { getTickArrayAddress } from "../utils";
import {
  TICK_ARRAY_SIZE,
  Tick,
  TickArray,
  getArrayStartIndex,
  getNextTickArrayStartIndex,
} from "../entities";

const FETCH_TICKARRAY_COUNT = 15;

export declare type PoolVars = {
  key: PublicKey;
  token0: PublicKey;
  token1: PublicKey;
  fee: number;
};

export class CacheDataProviderImpl implements CacheDataProvider {
  // @ts-ignore
  program: anchor.Program<AmmCore>;
  poolAddress: PublicKey;

  tickArrayCache: Map<number, TickArray | undefined>;

  // @ts-ignore
  constructor(program: anchor.Program<AmmCore>, poolAddress: PublicKey) {
    this.program = program;
    this.poolAddress = poolAddress;
    this.tickArrayCache = new Map();
  }

  /**
   * Caches ticks and bitmap accounts near the current price
   * @param tickCurrent The current pool tick
   * @param tickSpacing The pool tick spacing
   */
  async loadTickArrayCache(tickCurrent: number, tickSpacing: number) {
    const tickArraysToFetch = [];
    const startIndex = getArrayStartIndex(tickCurrent, tickSpacing);
    const [tickArrayAddress, _] = await getTickArrayAddress(
      this.poolAddress,
      this.program.programId,
      startIndex
    );
    tickArraysToFetch.push(tickArrayAddress);

    let lastStartIndex: number = startIndex;
    for (let i = 0; i < FETCH_TICKARRAY_COUNT / 2; i++) {
      const nextStartIndex = getNextTickArrayStartIndex(
        lastStartIndex,
        tickSpacing,
        true
      );
      const [tickArrayAddress, _] = await getTickArrayAddress(
        this.poolAddress,
        this.program.programId,
        nextStartIndex
      );
      tickArraysToFetch.push(tickArrayAddress);
      lastStartIndex = nextStartIndex;
    }
    lastStartIndex = startIndex;
    for (let i = 0; i < FETCH_TICKARRAY_COUNT / 2; i++) {
      const nextStartIndex = getNextTickArrayStartIndex(
        lastStartIndex,
        tickSpacing,
        false
      );
      const [tickArrayAddress, _] = await getTickArrayAddress(
        this.poolAddress,
        this.program.programId,
        nextStartIndex
      );
      tickArraysToFetch.push(tickArrayAddress);
      lastStartIndex = nextStartIndex;
    }

    const fetchedTickArrays =
      (await this.program.account.tickArrayState.fetchMultiple(
        tickArraysToFetch
      )) as (TickArray | null)[];

    for (const item of fetchedTickArrays) {
      if (item) {
        this.tickArrayCache.set(item.startTickIndex, item);
      }
    }
  }

  /**
   * Fetches the cached bitmap for the word
   * @param startIndex
   */
  getTickArray(startIndex: number): TickArray {
    let savedTickArray = this.tickArrayCache.get(startIndex);
    if (!savedTickArray) {
      throw new Error("tickArray not cached");
    }
    return savedTickArray;
  }

  /**
   * Finds the next initialized tick in the given word. Fetched bitmaps are saved in a
   * cache for quicker lookups in future.
   * @param tickIndex The current tick
   * @param zeroForOne Whether to look for a tick less than or equal to the current one, or a tick greater than or equal to
   * @param tickSpacing The tick spacing for the pool
   * @returns
   */
  async nextInitializedTick(
    tickIndex: number,
    tickSpacing: number,
    zeroForOne: boolean
  ): Promise<[Tick, PublicKey, number]> {
    let [nextTick, address, startIndex] =
      await this.nextInitializedTickInOneArray(
        tickIndex,
        tickSpacing,
        zeroForOne
      );
    while (nextTick == undefined || nextTick.liquidityGross.lten(0)) {
      const nextStartIndex = getNextTickArrayStartIndex(
        startIndex,
        tickSpacing,
        zeroForOne
      );
      const cachedTickArray = this.getTickArray(nextStartIndex);
      if (cachedTickArray == undefined) {
        throw new Error("No invaild tickArray cache");
      }
      [nextTick, address, startIndex] = await this.firstInitializedTickInOneArray(
        cachedTickArray,
        zeroForOne
      );
    }
    return [nextTick, address, startIndex];
  }

  async firstInitializedTickInOneArray(
    tickArray: TickArray,
    zeroForOne: boolean
  ): Promise<[Tick, PublicKey, number]> {
    let nextInitializedTick: Tick;
    if (zeroForOne) {
      let i = TICK_ARRAY_SIZE - 1;
      while (i >= 0) {
        const tickInArray = tickArray.ticks[i];
        if (tickInArray.liquidityGross.gtn(0)) {
          nextInitializedTick = tickInArray;
          break;
        }
        i = i - 1;
      }
    } else {
      let i = 0;
      while (i < TICK_ARRAY_SIZE) {
        const tickInArray = tickArray.ticks[i];
        if (tickInArray.liquidityGross.gtn(0)) {
          nextInitializedTick = tickInArray;
          break;
        }
        i = i + 1;
      }
    }
    const [tickArrayAddress, _] = await getTickArrayAddress(
      this.poolAddress,
      this.program.programId,
      tickArray.startTickIndex
    );
    return [nextInitializedTick, tickArrayAddress, tickArray.startTickIndex];
  }

  /**
   *
   * @param tickIndex
   * @param tickSpacing
   * @param zeroForOne
   * @returns
   */
  async nextInitializedTickInOneArray(
    tickIndex: number,
    tickSpacing: number,
    zeroForOne: boolean
  ): Promise<[Tick, PublicKey, number]> {
    const startIndex = getArrayStartIndex(tickIndex, tickSpacing);
    let isStartIndex = startIndex == tickIndex;
    let tickPositionInArray = Math.floor(
      (tickIndex - startIndex) / tickSpacing
    );
    const cachedTickArray = this.getTickArray(startIndex);
    let nextInitializedTick: Tick;
    if (zeroForOne) {
      if (isStartIndex) {
        tickPositionInArray = tickPositionInArray - 1;
      }
      while (tickPositionInArray >= 0) {
        const tickInArray = cachedTickArray.ticks[tickPositionInArray];
        if (tickInArray.liquidityGross.gtn(0)) {
          nextInitializedTick = tickInArray;
          break;
        }
        tickPositionInArray = tickPositionInArray - 1;
      }
    } else {
      if (isStartIndex) {
        tickPositionInArray = tickPositionInArray + 1;
      }
      while (tickPositionInArray < TICK_ARRAY_SIZE) {
        const tickInArray = cachedTickArray.ticks[tickPositionInArray];
        if (tickInArray.liquidityGross.gtn(0)) {
          nextInitializedTick = tickInArray;
          break;
        }
        tickPositionInArray = tickPositionInArray + 1;
      }
    }
    const [tickArrayAddress, _] = await getTickArrayAddress(
      this.poolAddress,
      this.program.programId,
      startIndex
    );
    return [
      nextInitializedTick,
      tickArrayAddress,
      cachedTickArray.startTickIndex,
    ];
  }
}
