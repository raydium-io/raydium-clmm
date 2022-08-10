import { PublicKey } from "@solana/web3.js";
import { Tick, TickArray } from "./tickArray";

export interface CacheDataProvider {
  /**
   *
   * @param tickArray
   */
  setTickArrayCache(tickArray: TickArray[]);

  /**
   *  Return the next tick and tickArray info
   * @param tick  The current tick
   * @param tickSpacing  The tick spacing of the pool
   * @param zeroForOne  Whether the next tick should be lte the current tick
   */
  nextInitializedTick(
    tick: number,
    tickSpacing: number,
    zeroForOne: boolean
  ): Promise<[Tick, PublicKey, number]>;
}
