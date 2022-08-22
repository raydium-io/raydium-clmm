import { BN } from "@project-serum/anchor";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import { AmmConfig, PoolState, StateFetcher } from "../states";
import { Context } from "../base";
import { NEGATIVE_ONE, SwapMath, Math as LibMath } from "../math";
import { CacheDataProviderImpl } from "./cacheProviderImpl";
import Decimal from "decimal.js";
import {
  TickArray,
  mergeTickArrayBitmap,
  getTickArrayOffsetInBitmapByTick,
  searchLowBitFromStart,
  searchHightBitFromStart,
  checkTickArrayIsInitialized,
} from "../entities";
import { getTickArrayAddress } from "../utils";

export class AmmPool {
  // public readonly fee: Fee;
  public readonly address: PublicKey;
  public readonly ctx: Context;
  public readonly cacheDataProvider: CacheDataProviderImpl;
  public readonly stateFetcher: StateFetcher;
  public poolState: PoolState;
  public ammConfig: AmmConfig;

  /**
   *
   * @param ctx
   * @param address
   * @param poolState
   * @param ammConfig
   * @param stateFetcher
   */
  public constructor(
    ctx: Context,
    address: PublicKey,
    poolState: PoolState,
    ammConfig: AmmConfig,
    stateFetcher: StateFetcher
  ) {
    this.ctx = ctx;
    this.stateFetcher = stateFetcher;
    this.address = address;
    this.poolState = poolState;
    this.ammConfig = ammConfig;
    this.cacheDataProvider = new CacheDataProviderImpl(ctx.program, address);
  }

  /**
   * Restset pool state from external
   * @param poolState
   */
  public setPoolState(poolState: PoolState) {
    this.poolState = poolState;
  }

  /**
   * Rsetset tick array datas from external
   * @param cachedTickArraies
   */
  public setTickArrayCache(cachedTickArraies: TickArray[]) {
    this.cacheDataProvider.setTickArrayCache(cachedTickArraies);
  }

  /**
   *
   * @returns
   */
  public async reloadPoolState(): Promise<PoolState> {
    const newState = await this.stateFetcher.getPoolState(this.address);
    this.poolState = newState;
    return this.poolState;
  }

  /**
   *
   * @param reloadPool
   */
  public async loadCache(reloadPool?: boolean) {
    if (reloadPool) {
      await this.reloadPoolState();
    }
    await this.cacheDataProvider.loadTickArrayCache(
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.tickArrayBitmap
    );
  }

  /**
   *
   * @param tokenMint
   * @returns
   */
  public isContain(tokenMint: PublicKey): boolean {
    return (
      tokenMint.equals(this.poolState.tokenMint0) ||
      tokenMint.equals(this.poolState.tokenMint1)
    );
  }

  /**
   *
   * @returns token0 price
   */
  public token0Price(): Decimal {
    return LibMath.x64ToDecimal(this.poolState.sqrtPriceX64);
  }

  /**
   *
   * @returns token1 price
   */
  public token1Price(): Decimal {
    return new Decimal(1).div(this.token0Price());
  }

  /**
   *
   * @param inputTokenMint
   * @param inputAmount
   * @param sqrtPriceLimitX64
   * @param reload  if true, reload pool state
   * @returns output token amount and the latest pool states
   */
  public async getOutputAmountAndRemainAccounts(
    inputTokenMint: PublicKey,
    inputAmount: BN,
    sqrtPriceLimitX64?: BN,
    reload?: boolean
  ): Promise<[BN, AccountMeta[]]> {
    if (!this.isContain(inputTokenMint)) {
      throw new Error("token is not in pool");
    }
    if (reload) {
      await this.reloadPoolState();
    }
    const zeroForOne = inputTokenMint.equals(this.poolState.tokenMint0);
    let allNeededAccounts: AccountMeta[] = [];
    let [isExist, nextStartIndex, nextAccountMeta] =
      await this.getNextInitializedTickArray(zeroForOne);
    if (!isExist) {
      throw new Error("Invalid tick array");
    }
    allNeededAccounts.push(nextAccountMeta);
    const {
      amountCalculated: outputAmount,
      sqrtPriceX64: updatedSqrtPriceX64,
      liquidity: updatedLiquidity,
      tickCurrent: updatedTick,
      accounts: reaminAccounts,
    } = await SwapMath.swapCompute(
      this.cacheDataProvider,
      zeroForOne,
      this.ammConfig.tradeFeeRate,
      this.poolState.liquidity,
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      inputAmount,
      nextStartIndex,
      sqrtPriceLimitX64
    );
    allNeededAccounts.push(...reaminAccounts);
    this.poolState.sqrtPriceX64 = updatedSqrtPriceX64;
    this.poolState.tickCurrent = updatedTick;
    this.poolState.liquidity = updatedLiquidity;
    return [outputAmount.mul(NEGATIVE_ONE), allNeededAccounts];
  }

  /**
   *  Base output swap
   * @param outputTokenMint
   * @param sqrtPriceLimitX64
   * @param reload if true, reload pool state
   * @returns input token amount and the latest pool states
   */
  public async getInputAmountAndAccounts(
    outputTokenMint: PublicKey,
    outputAmount: BN,
    sqrtPriceLimitX64?: BN,
    reload?: boolean
  ): Promise<[BN, AccountMeta[]]> {
    if (!this.isContain(outputTokenMint)) {
      throw new Error("token is not in pool");
    }
    if (reload) {
      this.reloadPoolState();
    }

    const zeroForOne = outputTokenMint.equals(this.poolState.tokenMint1);
    let allNeededAccounts: AccountMeta[] = [];
    let [isExist, nextStartIndex, nextAccountMeta] =
      await this.getNextInitializedTickArray(zeroForOne);
    if (!isExist) {
      throw new Error("Invalid tick array");
    }
    allNeededAccounts.push(nextAccountMeta);
    const {
      amountCalculated: inputAmount,
      sqrtPriceX64: updatedSqrtPriceX64,
      liquidity,
      tickCurrent,
      accounts: reaminAccounts,
    } = await SwapMath.swapCompute(
      this.cacheDataProvider,
      zeroForOne,
      this.ammConfig.tradeFeeRate,
      this.poolState.liquidity,
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      outputAmount.mul(NEGATIVE_ONE),
      nextStartIndex,
      sqrtPriceLimitX64
    );
    allNeededAccounts.push(...reaminAccounts);
    this.poolState.sqrtPriceX64 = updatedSqrtPriceX64;
    this.poolState.tickCurrent = tickCurrent;
    this.poolState.liquidity = liquidity;
    return [inputAmount, allNeededAccounts];
  }

  /**
   *
   * @returns
   */
  async getNextInitializedTickArray(
    zeroForOne: boolean
  ): Promise<[boolean, number, AccountMeta | undefined]> {
    const tickArrayBitmap = mergeTickArrayBitmap(
      this.poolState.tickArrayBitmap
    );
    let [isInitialized, startIndex] = checkTickArrayIsInitialized(
      tickArrayBitmap,
      this.poolState.tickCurrent,
      this.poolState.tickSpacing
    );
    if (isInitialized) {
      const [address, _] = await getTickArrayAddress(
        this.address,
        this.ctx.program.programId,
        startIndex
      );
      return [
        true,
        startIndex,
        {
          pubkey: address,
          isSigner: false,
          isWritable: true,
        },
      ];
    }
    let [isExist, nextStartIndex] =
      this.nextInitializedTickArrayStartIndex(zeroForOne);
    if (isExist) {
      const [address, _] = await getTickArrayAddress(
        this.address,
        this.ctx.program.programId,
        nextStartIndex
      );
      return [
        true,
        nextStartIndex,
        {
          pubkey: address,
          isSigner: false,
          isWritable: true,
        },
      ];
    }
    return [false, undefined, undefined];
  }

  /**
   *
   * @param zeroForOne
   * @returns
   */
  nextInitializedTickArrayStartIndex(zeroForOne: boolean): [boolean, number] {
    const tickArrayBitmap = mergeTickArrayBitmap(
      this.poolState.tickArrayBitmap
    );
    let currentOffset = getTickArrayOffsetInBitmapByTick(
      this.poolState.tickCurrent,
      this.poolState.tickSpacing
    );
    let result: number[] = [];
    if (zeroForOne) {
      result = searchLowBitFromStart(
        tickArrayBitmap,
        currentOffset - 1,
        0,
        1,
        this.poolState.tickSpacing
      );
    } else {
      result = searchHightBitFromStart(
        tickArrayBitmap,
        currentOffset,
        1024,
        1,
        this.poolState.tickSpacing
      );
    }
    if (result.length > 0) {
      return [true, result[0]];
    }
    return [false, 0];
  }
}
