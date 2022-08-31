import { BN } from "@project-serum/anchor";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import {
  AmmConfig,
  PoolState,
  RewardInfo,
  StateFetcher,
  TickArrayState,
} from "../states";
import { Context } from "../base";
import {
  NEGATIVE_ONE,
  SwapMath,
  SqrtPriceMath,
  getTickWithPriceAndTickspacing,
  MathUtil,
  Q64,
} from "../math";

import { CacheDataProviderImpl } from "./cacheProviderImpl";
import Decimal from "decimal.js";
import {
  TickArray,
  mergeTickArrayBitmap,
  getTickArrayOffsetInBitmapByTick,
  searchLowBitFromStart,
  searchHightBitFromStart,
  checkTickArrayIsInitialized,
  getAllInitializedTickArrayInfo,
} from "../entities";
import { getBlockTimestamp, getTickArrayAddress } from "../utils";
import { AmmConfigCache } from "./configCache";

export const REWARD_NUM = 3;

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
   * @param stateFetcher
   */
  public constructor(
    ctx: Context,
    address: PublicKey,
    stateFetcher: StateFetcher
  ) {
    this.ctx = ctx;
    this.stateFetcher = stateFetcher;
    this.address = address;
    this.poolState = null;
    this.ammConfig = null;
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
  public async loadPoolState(): Promise<PoolState> {
    this.poolState = await this.stateFetcher.getPoolState(this.address);
    this.ammConfig = await AmmConfigCache.getConfig(
      this.stateFetcher,
      this.poolState.ammConfig
    );
    await this.cacheDataProvider.loadTickArrayCache(
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.tickArrayBitmap
    );

    return this.poolState;
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
   * @returns token price
   */
  public tokenPrice(): Decimal {
    return SqrtPriceMath.sqrtPriceX64ToPrice(
      this.poolState.sqrtPriceX64,
      this.poolState.mintDecimals0,
      this.poolState.mintDecimals1
    );
  }

  public getRoundingTickWithPrice(price: Decimal): number {
    return getTickWithPriceAndTickspacing(
      price,
      this.poolState.tickSpacing,
      this.poolState.mintDecimals0,
      this.poolState.mintDecimals1
    );
  }

  public async getAllInitializedTickArrayStartIndex(): Promise<
    TickArrayState[]
  > {
    const allInitializedTickArrayInfo = await getAllInitializedTickArrayInfo(
      this.ctx.program.programId,
      this.address,
      mergeTickArrayBitmap(this.poolState.tickArrayBitmap),
      this.poolState.tickSpacing
    );
    let allInitializedTickArrayAddress: PublicKey[] = [];
    let allInitializedTickArrayIndex: number[] = [];
    for (const info of allInitializedTickArrayInfo) {
      allInitializedTickArrayAddress.push(info.tickArrayAddress);
      allInitializedTickArrayIndex.push(info.tickArrayStartIndex);
    }
    return await this.stateFetcher.getMultipleTickArrayState(
      allInitializedTickArrayAddress
    );
  }

  public async simulate_update_rewards(): Promise<RewardInfo[]> {
    let nextRewardInfos: RewardInfo[] = this.poolState.rewardInfos;
    const currTimestamp = await getBlockTimestamp(this.ctx.connection);
    for (let i = 0; i < REWARD_NUM; i++) {
      if (nextRewardInfos[i].tokenMint.equals(PublicKey.default)) {
        continue;
      }
      if (currTimestamp < nextRewardInfos[i].openTime.toNumber()) {
        continue;
      }

      let latestUpdateTime = new BN(currTimestamp);
      if (latestUpdateTime.gt(nextRewardInfos[i].endTime)) {
        latestUpdateTime = nextRewardInfos[i].endTime;
      }
      if (!this.poolState.liquidity.eqn(0)) {
        let timeDelta = latestUpdateTime.sub(nextRewardInfos[i].lastUpdateTime);

        let rewardGrowthDeltaX64 = MathUtil.mulDivFloor(
          timeDelta,
          nextRewardInfos[i].emissionsPerSecondX64,
          this.poolState.liquidity
        );
        nextRewardInfos[i].rewardGrowthGlobalX64 =
          nextRewardInfos[i].rewardGrowthGlobalX64.add(rewardGrowthDeltaX64);

        const rewardEmissionedDelta = MathUtil.mulDivFloor(
          timeDelta,
          nextRewardInfos[i].emissionsPerSecondX64,
          Q64
        );

        nextRewardInfos[i].rewardTotalEmissioned = nextRewardInfos[
          i
        ].rewardTotalEmissioned.add(rewardEmissionedDelta);
      }
      nextRewardInfos[i].lastUpdateTime = latestUpdateTime;
    }
    this.poolState.rewardInfos = nextRewardInfos;
    return nextRewardInfos;
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
      await this.loadPoolState();
    }

    if (!this.poolState) await this.loadPoolState();
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
      this.loadPoolState();
    }

    if (!this.poolState) await this.loadPoolState();

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
