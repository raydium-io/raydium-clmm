import * as anchor from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { BN } from "@project-serum/anchor";
import {
  tickPosition,
  buildTick,
  nextInitializedBit,
  generateBitmapWord,
} from "../entities";

import {TICK_SEED, u32ToBytes, u16ToBytes } from "../utils";
import { MAX_TICK, MIN_TICK } from "../math";
import { CacheDataProvider } from "../entities/cacheProvider";
import { getTickBitmapAddress,getTickAddress } from "../utils";
interface TickBitmap {
  word: BN[];
}

interface Tick {
  tick: number;
  liquidityNet: BN;
}

const FETCH_BITMAP_COUNT = 15;

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

  bitmapCache: Map<
    number,
    | {
        address: PublicKey;
        word: anchor.BN;
      }
    | undefined
  >;

  tickCache: Map<
    number,
    | {
        address: PublicKey;
        liquidityNet: BN;
      }
    | undefined
  >;

  // @ts-ignore
  constructor(program: anchor.Program<AmmCore>, poolAddress: PublicKey) {
    this.program = program;
    this.poolAddress = poolAddress;
    this.bitmapCache = new Map();
    this.tickCache = new Map();
  }

  /**
   * Caches ticks and bitmap accounts near the current price
   * @param tickCurrent The current pool tick
   * @param tickSpacing The pool tick spacing
   */
  async loadTickAndBitmapCache(tickCurrent: number, tickSpacing: number) {
    const { wordPos } = tickPosition(tickCurrent, tickSpacing);

    try {
      const bitmapsToFetch = [];
      const { wordPos: WORD_POS_MIN } = tickPosition(MIN_TICK, tickSpacing);
      const { wordPos: WORD_POS_MAX } = tickPosition(MAX_TICK, tickSpacing);
      const minWord = Math.max(wordPos - FETCH_BITMAP_COUNT, WORD_POS_MIN);
      const maxWord = Math.min(wordPos + FETCH_BITMAP_COUNT, WORD_POS_MAX);
      for (let i = minWord; i < maxWord; i++) {
        const [bitmapAddress, _] = await getTickBitmapAddress(
          this.poolAddress,
          this.program.programId,
          i
        );
        bitmapsToFetch.push(bitmapAddress);
      }
      const fetchedBitmaps =
        (await this.program.account.tickBitmapState.fetchMultiple(
          bitmapsToFetch
        )) as (TickBitmap | null)[];
      // console.log("fetchedBitmaps: ", fetchedBitmaps);
      const tickAddresses = [];
      for (let i = 0; i < maxWord - minWord; i++) {
        const currentWordPos = i + minWord;
        const wordArray = fetchedBitmaps[i]?.word;
        const word = wordArray ? generateBitmapWord(wordArray) : new BN(0);
        this.bitmapCache.set(currentWordPos, {
          address: bitmapsToFetch[i],
          word,
        });
        if (word && !word.eqn(0)) {
          for (let j = 0; j < 256; j++) {
            if (word.shrn(j).and(new BN(1)).eqn(1)) {
              const tick = ((currentWordPos << 8) + j) * tickSpacing;
              const [tickAddress, _] = await getTickAddress(
                this.poolAddress,
                this.program.programId,
                tick
              );
              tickAddresses.push(tickAddress);
            }
          }
        }
      }

      const fetchedTicks = (await this.program.account.tickState.fetchMultiple(
        tickAddresses
      )) as Tick[];
      for (const i in tickAddresses) {
        const { tick, liquidityNet } = fetchedTicks[i];
        this.tickCache.set(tick, {
          address: tickAddresses[i],
          liquidityNet: new BN(liquidityNet),
        });
      }
      // console.log("fetchedTicks: ", fetchedTicks);
    } catch (error) {
      console.log(error);
    }
  }

  getTick(tick: number): {
    address: anchor.web3.PublicKey;
    liquidityNet: BN;
  } {
    let savedTick = this.tickCache.get(tick);
    if (!savedTick) {
      throw new Error("Tick not cached");
    }

    return {
      address: savedTick.address,
      liquidityNet: savedTick.liquidityNet,
    };
  }

  /**
   * Fetches the cached bitmap for the word
   * @param wordPos
   */
  getBitmap(wordPos: number): {
    address: anchor.web3.PublicKey;
    word: anchor.BN;
  } {
    let savedBitmap = this.bitmapCache.get(wordPos);
    if (!savedBitmap) {
      throw new Error("Bitmap not cached");
    }

    return savedBitmap;
  }

  /**
   * Finds the next initialized tick in the given word. Fetched bitmaps are saved in a
   * cache for quicker lookups in future.
   * @param tick The current tick
   * @param lte Whether to look for a tick less than or equal to the current one, or a tick greater than or equal to
   * @param tickSpacing The tick spacing for the pool
   * @returns
   */
  nextInitializedTickWithinOneWord(
    tick: number,
    lte: boolean,
    tickSpacing: number
  ): [number, boolean, number, number, PublicKey] {
    let compressed = tick / tickSpacing;
    if (tick < 0 && tick % tickSpacing !== 0) {
      compressed -= 1;
    }
    if (!lte) {
      compressed += 1;
    }

    const { wordPos, bitPos } = tickPosition(tick, tickSpacing);
    const cachedBitmap = this.getBitmap(wordPos);

    const { next: nextBit, initialized } = nextInitializedBit(
      cachedBitmap.word,
      bitPos,
      lte
    );
    const nextTick = buildTick(wordPos, nextBit, tickSpacing);
    return [nextTick, initialized, wordPos, bitPos, cachedBitmap.address];
  }
}
