import { Price, Token } from "@raydium-io/raydium-sdk";
import { BN, web3 } from "@project-serum/anchor";
import { AccountMeta, MAX_SEED_LENGTH, PublicKey } from "@solana/web3.js";
import JSBI from "jsbi";
import { Fee, TICK_SPACINGS } from "../entities/fee";
import { PoolState, StateFetcher } from "../states";
import { Context } from "../entities";
import { SqrtPrice } from "../base";

import {
  NEGATIVE_ONE,
  ONE,
  Q64,
  ZERO,
  SwapMath,
  LiquidityMath,
  MIN_SQRT_PRICE_X64,
  MAX_SQRT_PRICE_X64,
  SqrtPriceMath,
  MIN_TICK,
  MAX_TICK,
  Math,
} from "../math";

import { CacheDataProvider } from "../entities/cacheProvider";
import { TokenAmount } from "../entities/tokenAmount";
import { CreatePoolAccounts } from "../instructions";
import { Program } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";
import Decimal from "decimal.js";

export class AmmPool {
  public readonly token0: Token;
  public readonly token1: Token;
  public readonly fee: Fee;

  public readonly address: PublicKey;
  public readonly program: Program<AmmCore>;
  public readonly tickDataProvider: CacheDataProvider;
  public readonly stateFetcher: StateFetcher;
  public poolState: PoolState;

  public constructor(
    program: Program<AmmCore>,
    address: PublicKey,
    token0: Token,
    token1: Token,
    poolState: PoolState,
    stateFetcher: StateFetcher,
    tickDataProvider: CacheDataProvider
  ) {
    this.token0 = token0;
    this.token1 = token1;
    this.address = address;
    this.program = program;
    this.stateFetcher = stateFetcher;
    this.tickDataProvider = tickDataProvider;
    if (poolState) {
      this.poolState = poolState;
    }
  }

  public static async createPool(
    program: Program<AmmCore>,
    initialPriceX64: BN,
    accounts: CreatePoolAccounts
  ) {
    return await program.methods
      .createPool(initialPriceX64)
      .accounts(accounts)
      .rpc();
  }

  public async reload(): Promise<PoolState> {
    this.poolState = await this.stateFetcher.getPoolState(this.address);
    return this.poolState;
  }

  public isContain(token: Token): boolean {
    return token.equals(this.token0) || token.equals(this.token1);
  }

  public get token0Price(): Decimal {
    return Math.x64ToDecimal(this.poolState.sqrtPriceX64);
  }

  public get token1Price(): Decimal {
    return new Decimal(1).div(this.token0Price);
  }

  /**
   * Base input swap
   * @param inputToken
   * @param sqrtPriceLimitX64
   * @param reload if true, reload pool state
   * @returns output token amount and the latest pool states
   */
  public async getOutputAmountAndRemainAccounts(
    inputToken: TokenAmount,
    sqrtPriceLimitX64?: BN,
    reload?: boolean
  ): Promise<[TokenAmount, PoolState, AccountMeta[]]> {
    if (!this.isContain(inputToken.currency)) {
      throw new Error("token is not in pool");
    }

    if (reload) {
      await this.reload();
    }

    const zeroForOne = inputToken.currency.equals(this.token0);
    const {
      amountCalculated: outputAmount,
      sqrtPriceX64: updatedSqrtPriceX64,
      liquidity: updatedLiquidity,
      tickCurrent: updatedTick,
      accounts,
    } = SwapMath.swapCompute(
      this.tickDataProvider,
      zeroForOne,
      this.poolState.feeRate,
      this.poolState.liquidity,
      this.poolState.tick,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      inputToken.amount,
      sqrtPriceLimitX64
    );
    const outputToken = zeroForOne ? this.token1 : this.token0;

    this.poolState.sqrtPriceX64 = updatedSqrtPriceX64;
    this.poolState.tick = updatedTick;
    this.poolState.liquidity = updatedLiquidity;
    return [
      new TokenAmount(outputToken, outputAmount.mul(NEGATIVE_ONE)),
      this.poolState,
      accounts,
    ];
  }

  /**
   *  Base output swap
   * @param outputToken
   * @param sqrtPriceLimitX64
   * @param reload if true, reload pool state
   * @returns input token amount and the latest pool states
   */
  public async getInputAmountAndAccounts(
    outputToken: TokenAmount,
    sqrtPriceLimitX64?: BN,
    reload?: boolean
  ): Promise<[TokenAmount, PoolState, AccountMeta[]]> {
    if (!this.isContain(outputToken.currency)) {
      throw new Error("token is not in pool");
    }

    if (reload) {
      this.reload();
    }

    const zeroForOne = outputToken.currency.equals(this.token1);
    const {
      amountCalculated: inputAmount,
      sqrtPriceX64: sqrtPriceX64,
      liquidity,
      tickCurrent,
      accounts,
    } = SwapMath.swapCompute(
      this.tickDataProvider,
      zeroForOne,
      this.poolState.feeRate,
      this.poolState.liquidity,
      this.poolState.tick,
      this.poolState.tickSpacing,
      this.poolState.sqrtPriceX64,
      outputToken.amount.mul(NEGATIVE_ONE),
      sqrtPriceLimitX64
    );
    const inputToken = zeroForOne ? this.token0 : this.token1;
    this.poolState.sqrtPriceX64 = sqrtPriceX64;
    this.poolState.tick = tickCurrent;
    this.poolState.liquidity = liquidity;

    return [new TokenAmount(inputToken, inputAmount), this.poolState, accounts];
  }
}
