import { Price, Token } from "@raydium-io/raydium-sdk";
import { web3 } from "@project-serum/anchor";
import { AccountMeta } from "@solana/web3.js";
import JSBI from "jsbi";
import invariant from "tiny-invariant";
import { FeeAmount, TICK_SPACINGS } from "./fee";
// import { Currency } from "../base";
import {
  NEGATIVE_ONE,
  ONE,
  Q64,
  ZERO,
  SwapMath,
  TickMath,
  LiquidityMath,
} from "../math";

import { NoTickDataProvider, TickDataProvider } from "./tickDataProvider";
import { TokenAmount } from "./tokenAmount";
import { getPoolAddress } from "../utils";

export interface StepComputations {
  sqrtPriceStartX64: JSBI;
  tickNext: number;
  initialized: boolean;
  sqrtPriceNextX64: JSBI;
  amountIn: JSBI;
  amountOut: JSBI;
  feeAmount: JSBI;
}

const NO_TICK_DATA_PROVIDER_DEFAULT = new NoTickDataProvider();

export class Pool {
  public readonly tokenA: Token;
  public readonly tokenB: Token;
  public readonly fee: FeeAmount;
  public readonly sqrtRatioX64: JSBI;
  public readonly liquidity: JSBI;
  public readonly tickCurrent: number;
  public readonly tickDataProvider: TickDataProvider;

  private _token0Price?: Price;
  private _token1Price?: Price;

  public getAddress(
    ammConfig: web3.PublicKey,
    programId: web3.PublicKey,
  ): Promise<web3.PublicKey> {
    return getPoolAddress(
      ammConfig,
      this.tokenA.mint,
      this.tokenB.mint,
      programId,
      this.fee
    )[0];
  }

  public constructor(
    tokenA: Token,
    tokenB: Token,
    fee: FeeAmount,
    sqrtPriceX64: JSBI,
    liquidity: JSBI,
    tickCurrent: number,
    tickDataProvider: TickDataProvider = NO_TICK_DATA_PROVIDER_DEFAULT
  ) {
    invariant(Number.isInteger(fee) && fee < 1_000_000, "FEE");

    const tickCurrentSqrtPriceX64 =
      TickMath.getSqrtPriceX64FromTick(tickCurrent);
    const nextTickSqrtPriceX64 = TickMath.getSqrtPriceX64FromTick(
      tickCurrent + 1
    );
    invariant(
      JSBI.greaterThanOrEqual(sqrtPriceX64, tickCurrentSqrtPriceX64) &&
        JSBI.lessThanOrEqual(sqrtPriceX64, nextTickSqrtPriceX64),
      "PRICE_BOUNDS"
    );

    if (tokenA.mint < tokenB.mint) {
      this.tokenA  = tokenA
      this.tokenB =  tokenB
    } else {
      this.tokenA  = tokenB
      this.tokenB =  tokenA
    }
    this.fee = fee;
    this.sqrtRatioX64 = sqrtPriceX64;
    this.liquidity = liquidity;
    this.tickCurrent = tickCurrent;
    this.tickDataProvider = tickDataProvider;
  }

  public isContain(token: Token): boolean {
    return (
      token.equals(this.tokenA) || token.equals(this.tokenB)
    );
  }

  public get token0Price(): Price {
    return (
      this._token0Price ??
      (this._token0Price = new Price(
        this.tokenA,
        this.tokenB,
        Q64,
        JSBI.multiply(this.sqrtRatioX64, this.sqrtRatioX64)
      ))
    );
  }

  public get token1Price(): Price {
    return (
      this._token1Price ??
      (this._token1Price = new Price(
        this.tokenB,
        this.tokenA,
        JSBI.multiply(this.sqrtRatioX64, this.sqrtRatioX64),
        Q64
      ))
    );
  }

  public priceOf(token: Token): Price{
    invariant(this.isContain(token), "TOKEN");
    return token.equals(this.tokenA) ? this.token0Price : this.token1Price;
  }

  public getOutputAmount(
    inputToken: TokenAmount,
    sqrtPriceLimitX64?: JSBI
  ): [TokenAmount, Pool, AccountMeta[]] {
    invariant(this.isContain(inputToken.currency), "TOKEN");

    const zeroForOne = inputToken.currency.equals(this.tokenA);

    const {
      amountCalculated: outputAmount,
      sqrtRatioX64: sqrtRatioX32,
      liquidity,
      tickCurrent,
      accounts,
    } = this.swap(zeroForOne, inputToken.amount, sqrtPriceLimitX64);
    const outputToken = zeroForOne ? this.tokenB : this.tokenA;
    return [
      new TokenAmount(outputToken, JSBI.multiply(outputAmount, NEGATIVE_ONE)),
      new Pool(
        this.tokenA,
        this.tokenB,
        this.fee,
        sqrtRatioX32,
        liquidity,
        tickCurrent,
        this.tickDataProvider
      ),
      accounts,
    ];
  }

  public getInputAmount(
    outputToken: TokenAmount,
    sqrtPriceLimitX64?: JSBI
  ): [TokenAmount, Pool] {
    invariant(this.isContain(outputToken.currency), "TOKEN");

    const zeroForOne = outputToken.currency.equals(this.tokenB);

    const {
      amountCalculated: inputAmount,
      sqrtRatioX64: sqrtRatioX32,
      liquidity,
      tickCurrent,
    } = this.swap(
      zeroForOne,
      JSBI.multiply(outputToken.amount, NEGATIVE_ONE),
      sqrtPriceLimitX64
    );
    const inputToken = zeroForOne ? this.tokenA : this.tokenB;
    return [
      new TokenAmount(inputToken, inputAmount),
      new Pool(
        this.tokenA,
        this.tokenB,
        this.fee,
        sqrtRatioX32,
        liquidity,
        tickCurrent,
        this.tickDataProvider
      ),
    ];
  }

  private swap(
    zeroForOne: boolean,
    amountSpecified: JSBI,
    sqrtPriceLimitX64?: JSBI
  ): {
    amountCalculated: JSBI;
    sqrtRatioX64: JSBI;
    liquidity: JSBI;
    tickCurrent: number;
    accounts: AccountMeta[];
  } {
    invariant(JSBI.notEqual(amountSpecified, ZERO), "AMOUNT_LESS_THAN_0");

    if (!sqrtPriceLimitX64)
      sqrtPriceLimitX64 = zeroForOne
        ? JSBI.add(TickMath.MIN_SQRT_RATIO, ONE)
        : JSBI.subtract(TickMath.MAX_SQRT_RATIO, ONE);

    if (zeroForOne) {
      invariant(
        JSBI.greaterThan(sqrtPriceLimitX64, TickMath.MIN_SQRT_RATIO),
        "RATIO_MIN"
      );
      invariant(
        JSBI.lessThan(sqrtPriceLimitX64, this.sqrtRatioX64),
        "RATIO_CURRENT"
      );
    } else {
      invariant(
        JSBI.lessThan(sqrtPriceLimitX64, TickMath.MAX_SQRT_RATIO),
        "RATIO_MAX"
      );
      invariant(
        JSBI.greaterThan(sqrtPriceLimitX64, this.sqrtRatioX64),
        "RATIO_CURRENT"
      );
    }
    const exactInput = JSBI.greaterThanOrEqual(amountSpecified, ZERO);

    const state = {
      amountSpecifiedRemaining: amountSpecified,
      amountCalculated: ZERO,
      sqrtPriceX64: this.sqrtRatioX64,
      tick: this.tickCurrent,
      accounts: [] as AccountMeta[],
      liquidity: this.liquidity,
    };
    console.log(
      "swap begin, tick: ",
      this.tickCurrent,
      " sqrtPrice: ",
      this.sqrtRatioX64.toString()
    );
    let lastSavedWordPos: number | undefined;

    let loopCount = 0;
    // loop across ticks until input liquidity is consumed, or the limit price is reached
    while (
      JSBI.notEqual(state.amountSpecifiedRemaining, ZERO) &&
      state.sqrtPriceX64 != sqrtPriceLimitX64 &&
      state.tick < TickMath.MAX_TICK &&
      state.tick > TickMath.MIN_TICK
    ) {
      if (loopCount > 8) {
        throw Error("account limit");
      }
      console.log(
        " --------------------------------------------------------------------------"
      );
      let step: Partial<StepComputations> = {};
      step.sqrtPriceStartX64 = state.sqrtPriceX64;

      // save the bitmap, and the tick account if it is initialized
      const nextInitTick =
        this.tickDataProvider.nextInitializedTickWithinOneWord(
          state.tick,
          zeroForOne,
          this.tickSpacing
        );

      step.tickNext = nextInitTick[0];
      step.initialized = nextInitTick[1];
      const wordPos = nextInitTick[2];
      const bitmapAddress = nextInitTick[4];

      if (lastSavedWordPos !== wordPos) {
        state.accounts.push({
          pubkey: bitmapAddress,
          isWritable: false,
          isSigner: false,
        });
        lastSavedWordPos = wordPos;
      }

      if (step.tickNext < TickMath.MIN_TICK) {
        step.tickNext = TickMath.MIN_TICK;
      } else if (step.tickNext > TickMath.MAX_TICK) {
        step.tickNext = TickMath.MAX_TICK;
      }

      step.sqrtPriceNextX64 = TickMath.getSqrtPriceX64FromTick(step.tickNext);
      console.log(
        "state.tick: ",
        state.tick,
        " state.sqrtPriceX64: ",
        state.sqrtPriceX64.toString(),
        "step.sqrtPriceNextX64: ",
        step.sqrtPriceNextX64.toString(),
        "step.tickNext:",
        step.tickNext,
        " state.liquidity:",
        state.liquidity.toString()
      );
      [state.sqrtPriceX64, step.amountIn, step.amountOut, step.feeAmount] =
        SwapMath.computeSwapStep(
          state.sqrtPriceX64,
          (
            zeroForOne
              ? JSBI.lessThan(step.sqrtPriceNextX64, sqrtPriceLimitX64)
              : JSBI.greaterThan(step.sqrtPriceNextX64, sqrtPriceLimitX64)
          )
            ? sqrtPriceLimitX64
            : step.sqrtPriceNextX64,
          state.liquidity,
          state.amountSpecifiedRemaining,
          this.fee
        );
      console.log(
        "step.amountIn:",
        step.amountIn.toString(),
        "step.amountOut",
        step.amountOut.toString()
      );
      if (exactInput) {
        // subtract the input amount. The loop exits if remaining amount becomes 0
        state.amountSpecifiedRemaining = JSBI.subtract(
          state.amountSpecifiedRemaining,
          JSBI.add(step.amountIn, step.feeAmount)
        );
        state.amountCalculated = JSBI.subtract(
          state.amountCalculated,
          step.amountOut
        );
      } else {
        state.amountSpecifiedRemaining = JSBI.add(
          state.amountSpecifiedRemaining,
          step.amountOut
        );
        state.amountCalculated = JSBI.add(
          state.amountCalculated,
          JSBI.add(step.amountIn, step.feeAmount)
        );
      }
      console.log(
        "after swap, state.sqrtPriceX64:",
        state.sqrtPriceX64.toString(),
        " state.amountCalculated: ",
        state.amountCalculated.toString(),
        "  state.amountSpecifiedRemaining: ",
        state.amountSpecifiedRemaining.toString()
      );
      // TODO
      if (JSBI.equal(state.sqrtPriceX64, step.sqrtPriceNextX64)) {
        // if the tick is initialized, run the tick transition
        if (step.initialized) {
          const tickNext = this.tickDataProvider.getTick(step.tickNext);
          // push the crossed tick to accounts array
          state.accounts.push({
            pubkey: tickNext.address,
            isWritable: true,
            isSigner: false,
          });
          // get the liquidity at this tick
          let liquidityNet = tickNext.liquidityNet;
          // if we're moving leftward, we interpret liquidityNet as the opposite sign
          // safe because liquidityNet cannot be type(int128).min
          if (zeroForOne)
            liquidityNet = JSBI.multiply(liquidityNet, NEGATIVE_ONE);

          state.liquidity = LiquidityMath.addDelta(
            state.liquidity,
            liquidityNet
          );
        }
        state.tick = zeroForOne ? step.tickNext - 1 : step.tickNext;
      } else if (state.sqrtPriceX64 != step.sqrtPriceStartX64) {
        // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
        state.tick = TickMath.getTickFromSqrtPriceX64(state.sqrtPriceX64);
      }
      console.log(
        "state.sqrtPriceX64:",
        state.sqrtPriceX64.toString(),
        " step.sqrtPriceNextX64: ",
        step.sqrtPriceNextX64.toString(),
        " state.tick: ",
        state.tick
      );

      console.log(
        " --------------------------------------------------------------------------"
      );
      ++loopCount;
    }

    return {
      amountCalculated: state.amountCalculated,
      sqrtRatioX64: state.sqrtPriceX64,
      liquidity: state.liquidity,
      tickCurrent: state.tick,
      accounts: state.accounts,
    };
  }

  public get tickSpacing(): number {
    return TICK_SPACINGS[this.fee];
  }
}
