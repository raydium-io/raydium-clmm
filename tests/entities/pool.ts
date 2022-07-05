import { Price, Token } from '@cykura/sdk-core'
import { web3 } from '@project-serum/anchor'
import { AccountMeta } from '@solana/web3.js'
import JSBI from 'jsbi'
import invariant from 'tiny-invariant'
import {FeeAmount, TICK_SPACINGS } from './fee'
import { NEGATIVE_ONE, ONE, Q64, ZERO } from '../constants'
import { LiquidityMath } from '../math/liquidityMath'
import { SwapMath } from '../math/swapMath'
import { TickMath } from '../math/tickMath'
import { NoTickDataProvider, TickDataProvider } from './tickDataProvider'
import {TokenAmount} from "./tokenAmount"

export interface StepComputations {
  sqrtPriceStartX64: JSBI
  tickNext: number
  initialized: boolean
  sqrtPriceNextX64: JSBI
  amountIn: JSBI
  amountOut: JSBI
  feeAmount: JSBI
}

/**
 * By default, pools will not allow operations that require ticks.
 */
const NO_TICK_DATA_PROVIDER_DEFAULT = new NoTickDataProvider()

/**
 * Represents a V3 pool
 */
export class Pool {
  public readonly token0: Token
  public readonly token1: Token
  public readonly fee: FeeAmount
  public readonly sqrtRatioX64: JSBI
  public readonly liquidity: JSBI
  public readonly tickCurrent: number
  public readonly tickDataProvider: TickDataProvider

  private _token0Price?: Price<Token, Token>
  private _token1Price?: Price<Token, Token>

//   public static getAddress(
//     tokenA: Token,
//     tokenB: Token,
//     fee: FeeAmount,
//   ): Promise<web3.PublicKey> {
//     // TODO
//     // return computePoolAddress({ factoryAddress: "", fee, tokenA, tokenB })
//   }

  /**
   * Construct a pool
   * @param tokenA One of the tokens in the pool
   * @param tokenB The other token in the pool
   * @param fee The fee in hundredths of a bips of the input amount of every swap that is collected by the pool
   * @param sqrtRatioX64 The sqrt of the current ratio of amounts of token1 to token0
   * @param liquidity The current value of in range liquidity
   * @param tickCurrent The current tick of the pool
   * @param tickDataProvider The current state of the pool ticks or a data provider that can return tick data
   */
  public constructor(
    tokenA: Token,
    tokenB: Token,
    fee: FeeAmount,
    sqrtRatioX64: JSBI,
    liquidity: JSBI,
    tickCurrent: number,
    tickDataProvider: TickDataProvider = NO_TICK_DATA_PROVIDER_DEFAULT
  ) {
    invariant(Number.isInteger(fee) && fee < 1_000_000, 'FEE')

    const tickCurrentSqrtRatioX32 = TickMath.getSqrtRatioAtTick(tickCurrent)
    const nextTickSqrtRatioX32 = TickMath.getSqrtRatioAtTick(tickCurrent + 1)
    invariant(
      JSBI.greaterThanOrEqual(sqrtRatioX64, tickCurrentSqrtRatioX32) &&
      JSBI.lessThanOrEqual(sqrtRatioX64, nextTickSqrtRatioX32),
      'PRICE_BOUNDS'
    )
      // always create a copy of the list since we want the pool's tick list to be immutable
      ;[this.token0, this.token1] = tokenA.sortsBefore(tokenB) ? [tokenA, tokenB] : [tokenB, tokenA]
    this.fee = fee
    this.sqrtRatioX64 = sqrtRatioX64
    this.liquidity = liquidity
    this.tickCurrent = tickCurrent
    this.tickDataProvider = tickDataProvider
  }

  /**
   * Returns true if the token is either token0 or token1
   * @param token The token to check
   * @returns True if token is either token0 or token
   */
  public involvesToken(token: Token): boolean {
    return token.equals(this.token0) || token.equals(this.token1)
  }

  /**
   * Returns the current mid price of the pool in terms of token0, i.e. the ratio of token1 over token0
   */
  public get token0Price(): Price<Token, Token> {
    return (
      this._token0Price ??
      (this._token0Price = new Price(
        this.token0,
        this.token1,
        Q64,
        JSBI.multiply(this.sqrtRatioX64, this.sqrtRatioX64)
      ))
    )
  }

  /**
   * Returns the current mid price of the pool in terms of token1, i.e. the ratio of token0 over token1
   */
  public get token1Price(): Price<Token, Token> {
    return (
      this._token1Price ??
      (this._token1Price = new Price(
        this.token1,
        this.token0,
        JSBI.multiply(this.sqrtRatioX64, this.sqrtRatioX64),
        Q64
      ))
    )
  }

  /**
   * Return the price of the given token in terms of the other token in the pool.
   * @param token The token to return price of
   * @returns The price of the given token, in terms of the other.
   */
  public priceOf(token: Token): Price<Token, Token> {
    invariant(this.involvesToken(token), 'TOKEN')
    return token.equals(this.token0) ? this.token0Price : this.token1Price
  }

  /**
   * Returns the chain ID of the tokens in the pool.
   */
  public get chainId(): number {
    return this.token0.chainId
  }

  /**
   * Given an input amount of a token, return the computed output amount, and a pool with state updated after the trade
   * @param inputToken The input amount for which to quote the output amount
   * @param sqrtPriceLimitX64 The Q32.32 sqrt price limit
   * @returns The output amount and the pool with updated state
   */
  public getOutputAmount(
    inputToken: TokenAmount,
    sqrtPriceLimitX64?: JSBI
  ): [TokenAmount, Pool, AccountMeta[]] {
    invariant(this.involvesToken(inputToken.currency), 'TOKEN')

    const zeroForOne =inputToken.currency.equals(this.token0)
    
    const { amountCalculated: outputAmount, sqrtRatioX64: sqrtRatioX32, liquidity, tickCurrent, accounts } = this.swap(
      zeroForOne,
      inputToken.amount,
      sqrtPriceLimitX64
    )
    const outputToken = zeroForOne ? this.token1 : this.token0
    return [
      new TokenAmount(outputToken, JSBI.multiply(outputAmount, NEGATIVE_ONE)),
      new Pool(this.token0, this.token1, this.fee, sqrtRatioX32, liquidity, tickCurrent, this.tickDataProvider),
      accounts
    ]
  }

  /**
   * Given a desired output amount of a token, return the computed input amount and a pool with state updated after the trade
   * @param outputToken the output amount for which to quote the input amount
   * @param sqrtPriceLimitX64 The Q64.64 sqrt price limit. If zero for one, the price cannot be less than this value after the swap. If one for zero, the price cannot be greater than this value after the swap
   * @returns The input amount and the pool with updated state
   */
  public getInputAmount(
    outputToken: TokenAmount,
    sqrtPriceLimitX64?: JSBI
  ): [TokenAmount, Pool] {
    invariant(this.involvesToken(outputToken.currency), 'TOKEN')

    const zeroForOne = outputToken.currency.equals(this.token1)

    const { amountCalculated: inputAmount, sqrtRatioX64: sqrtRatioX32, liquidity, tickCurrent } = this.swap(
      zeroForOne,
      JSBI.multiply(outputToken.amount, NEGATIVE_ONE),
      sqrtPriceLimitX64
    )
    const inputToken = zeroForOne ? this.token0 : this.token1
    return [
      new TokenAmount(inputToken, inputAmount),
      new Pool(this.token0, this.token1, this.fee, sqrtRatioX32, liquidity, tickCurrent, this.tickDataProvider)
    ]
  }

  /**
   * Simulate a swap
   * @param zeroForOne Whether the amount in is token0 or token1
   * @param amountSpecified The amount of the swap, which implicitly configures the swap as exact input (positive), or exact output (negative)
   * @param sqrtPriceLimitX64 The Q32.32 sqrt price limit. If zero for one, the price cannot be less than this value after the swap. If one for zero, the price cannot be greater than this value after the swap
   * @returns amountCalculated
   * @returns sqrtRatioX32
   * @returns liquidity
   * @returns tickCurrent
   * @returns accounts Tick accounts flipped and bitmaps traversed
   */
  private swap(
    zeroForOne: boolean,
    amountSpecified: JSBI,
    sqrtPriceLimitX64?: JSBI
  ): {
    amountCalculated: JSBI
    sqrtRatioX64: JSBI
    liquidity: JSBI
    tickCurrent: number
    accounts: AccountMeta[]
  } {
    invariant(JSBI.notEqual(amountSpecified, ZERO), 'AMOUNT_LESS_THAN_0')

    if (!sqrtPriceLimitX64)
      sqrtPriceLimitX64 = zeroForOne
        ? JSBI.add(TickMath.MIN_SQRT_RATIO, ONE)
        : JSBI.subtract(TickMath.MAX_SQRT_RATIO, ONE)

    if (zeroForOne) {
      invariant(JSBI.greaterThan(sqrtPriceLimitX64, TickMath.MIN_SQRT_RATIO), 'RATIO_MIN')
      invariant(JSBI.lessThan(sqrtPriceLimitX64, this.sqrtRatioX64), 'RATIO_CURRENT')
    } else {
      invariant(JSBI.lessThan(sqrtPriceLimitX64, TickMath.MAX_SQRT_RATIO), 'RATIO_MAX')
      invariant(JSBI.greaterThan(sqrtPriceLimitX64, this.sqrtRatioX64), 'RATIO_CURRENT')
    }
    const exactInput = JSBI.greaterThanOrEqual(amountSpecified, ZERO)

    const state = {
      amountSpecifiedRemaining: amountSpecified,
      amountCalculated: ZERO,
      sqrtPriceX64: this.sqrtRatioX64,
      tick: this.tickCurrent,
      accounts: [] as AccountMeta[],
      liquidity: this.liquidity
    }
    console.log("swap begin, tick: ", this.tickCurrent, " sqrtPrice: ", this.sqrtRatioX64.toString())
    let lastSavedWordPos: number | undefined

    let loopCount = 0
    // loop across ticks until input liquidity is consumed, or the limit price is reached
    while (
      JSBI.notEqual(state.amountSpecifiedRemaining, ZERO) &&
      state.sqrtPriceX64 != sqrtPriceLimitX64 &&
      state.tick < TickMath.MAX_TICK &&
      state.tick > TickMath.MIN_TICK
    ) {
      if (loopCount > 8) {
        throw Error('account limit')
      }
      console.log(" --------------------------------------------------------------------------")
      let step: Partial<StepComputations> = {}
      step.sqrtPriceStartX64 = state.sqrtPriceX64

      // save the bitmap, and the tick account if it is initialized
      const nextInitTick = this.tickDataProvider.nextInitializedTickWithinOneWord(
        state.tick,
        zeroForOne,
        this.tickSpacing
      )
    
      step.tickNext = nextInitTick[0]
      step.initialized = nextInitTick[1]
      const wordPos = nextInitTick[2]
      const bitmapAddress = nextInitTick[4]

      if (lastSavedWordPos !== wordPos) {
        state.accounts.push({
          pubkey: bitmapAddress,
          isWritable: false,
          isSigner: false
        })
        lastSavedWordPos = wordPos
      }

      if (step.tickNext < TickMath.MIN_TICK) {
        step.tickNext = TickMath.MIN_TICK
      } else if (step.tickNext > TickMath.MAX_TICK) {
        step.tickNext = TickMath.MAX_TICK
      }

      step.sqrtPriceNextX64 = TickMath.getSqrtRatioAtTick(step.tickNext);
      console.log("state.tick: ", state.tick," state.sqrtPriceX64: ", state.sqrtPriceX64.toString(),"step.sqrtPriceNextX64: ", step.sqrtPriceNextX64.toString(), "step.tickNext:",step.tickNext," state.liquidity:", state.liquidity.toString());
      [state.sqrtPriceX64, step.amountIn, step.amountOut, step.feeAmount] = SwapMath.computeSwapStep(
          state.sqrtPriceX64,
          (zeroForOne
            ? JSBI.lessThan(step.sqrtPriceNextX64, sqrtPriceLimitX64)
            : JSBI.greaterThan(step.sqrtPriceNextX64, sqrtPriceLimitX64))
            ? sqrtPriceLimitX64
            : step.sqrtPriceNextX64,
          state.liquidity,
          state.amountSpecifiedRemaining,
          this.fee
        )
      console.log("step.amountIn:", step.amountIn.toString(),"step.amountOut", step.amountOut.toString())
      if (exactInput) {
        // subtract the input amount. The loop exits if remaining amount becomes 0
        state.amountSpecifiedRemaining = JSBI.subtract(
          state.amountSpecifiedRemaining,
          JSBI.add(step.amountIn, step.feeAmount)
        )
        state.amountCalculated = JSBI.subtract(state.amountCalculated, step.amountOut)
      } else {
        state.amountSpecifiedRemaining = JSBI.add(state.amountSpecifiedRemaining, step.amountOut)
        state.amountCalculated = JSBI.add(state.amountCalculated, JSBI.add(step.amountIn, step.feeAmount))
      }
      console.log("after swap, state.sqrtPriceX64:",state.sqrtPriceX64.toString()," state.amountCalculated: ", state.amountCalculated.toString(), "  state.amountSpecifiedRemaining: ",  state.amountSpecifiedRemaining.toString())
      // TODO
      if (JSBI.equal(state.sqrtPriceX64, step.sqrtPriceNextX64)) {
        // if the tick is initialized, run the tick transition
        if (step.initialized) {
          const tickNext = this.tickDataProvider.getTick(step.tickNext)
          // push the crossed tick to accounts array
          state.accounts.push({
            pubkey: tickNext.address,
            isWritable: true,
            isSigner: false
          })
          // get the liquidity at this tick
          let liquidityNet = tickNext.liquidityNet
          // if we're moving leftward, we interpret liquidityNet as the opposite sign
          // safe because liquidityNet cannot be type(int128).min
          if (zeroForOne) liquidityNet = JSBI.multiply(liquidityNet, NEGATIVE_ONE)

          state.liquidity = LiquidityMath.addDelta(state.liquidity, liquidityNet)
        }
        state.tick = zeroForOne ? step.tickNext - 1 : step.tickNext

      } else if (state.sqrtPriceX64 != step.sqrtPriceStartX64) {
        // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
        state.tick = TickMath.getTickAtSqrtRatio(state.sqrtPriceX64)
      }
      console.log("state.sqrtPriceX64:",state.sqrtPriceX64.toString()," step.sqrtPriceNextX64: ", step.sqrtPriceNextX64.toString()," state.tick: ", state.tick)

      console.log(" --------------------------------------------------------------------------")
      ++loopCount
    }

    return {
      amountCalculated: state.amountCalculated,
      sqrtRatioX64: state.sqrtPriceX64,
      liquidity: state.liquidity,
      tickCurrent: state.tick,
      accounts: state.accounts
    }
  }

  public get tickSpacing(): number {
    return TICK_SPACINGS[this.fee]
  }
}
