import { Token } from "@raydium-io/raydium-sdk";
import { BN } from "@project-serum/anchor";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import { AmmConfig, PoolState, StateFetcher } from "../states";
import { Context } from "../base";
import { NEGATIVE_ONE, SwapMath, Math } from "../math";
import { CacheDataProviderImpl } from "./cacheProviderImpl";
import Decimal from "decimal.js";

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
   * @param stateFetcher
   * @param cacheDataProvider
   */
  public constructor(
    ctx: Context,
    address: PublicKey,
    poolState: PoolState,
    ammConfig: AmmConfig,
    stateFetcher: StateFetcher,
  ) {
    this.ctx = ctx;
    this.stateFetcher = stateFetcher;
    this.address = address;
    this.poolState = poolState;
    this.ammConfig = ammConfig;
    this.cacheDataProvider = new CacheDataProviderImpl(
      ctx.program,
      address
    );
  }

  public async reloadPoolState(): Promise<PoolState> {
    const newState = await this.stateFetcher.getPoolState(this.address);
    this.poolState = newState;
    return this.poolState;
  }

  public async reloadCache() {
    await this.cacheDataProvider.loadTickArrayCache(
      this.poolState.tickCurrent,
      this.poolState.tickSpacing
    );
  }

  public isContain(tokenMint: PublicKey): boolean {
    return (
      tokenMint.equals(this.poolState.tokenMint0) ||
      tokenMint.equals(this.poolState.tokenMint1)
    );
  }

  public token0Price(): Decimal {
    return Math.x64ToDecimal(this.poolState.sqrtPriceX64);
  }

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
    const {
      amountCalculated: outputAmount,
      sqrtPriceX64: updatedSqrtPriceX64,
      liquidity: updatedLiquidity,
      tickCurrent: updatedTick,
      accounts,
    } = await SwapMath.swapCompute(
      this.cacheDataProvider,
      zeroForOne,
      this.ammConfig.tradeFeeRate,
      this.poolState.liquidity,
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      inputAmount,
      sqrtPriceLimitX64
    );

    this.poolState.sqrtPriceX64 = updatedSqrtPriceX64;
    this.poolState.tickCurrent = updatedTick;
    this.poolState.liquidity = updatedLiquidity;
    return [outputAmount.mul(NEGATIVE_ONE), accounts];
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
    const {
      amountCalculated: inputAmount,
      sqrtPriceX64: updatedSqrtPriceX64,
      liquidity,
      tickCurrent,
      accounts,
    } = await SwapMath.swapCompute(
      this.cacheDataProvider,
      zeroForOne,
      this.ammConfig.tradeFeeRate,
      this.poolState.liquidity,
      this.poolState.tickCurrent,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      outputAmount.mul(NEGATIVE_ONE),
      sqrtPriceLimitX64
    );
    this.poolState.sqrtPriceX64 = updatedSqrtPriceX64;
    this.poolState.tickCurrent = tickCurrent;
    this.poolState.liquidity = liquidity;

    return [inputAmount, accounts];
  }
}
