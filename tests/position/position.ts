import { PublicKey,TransactionSignature } from "@solana/web3.js";
import {PositionState, StateFetcher } from "../states";

import {
  OpenPositionAccounts,
  OpenPositionArgs,
  ClosePositionAccounts,
  IncreaseLiquidityAccounts,
  IncreaseLiquidityArgs,
  DecreaseLiquidityAccounts,
  DecreaseLiquidityArgs,
  CollectFeeAccounts,
  CollectFeeArgs,
  CollectRewardsAccounts,
} from "../instructions";

import { Program } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";
import Decimal from "decimal.js";

export class Position {
  public readonly program: Program<AmmCore>;
  public readonly stateFetcher: StateFetcher;

  public readonly address: PublicKey;
  public positionState: PositionState;

  public constructor(
    address: PublicKey,
    program: Program<AmmCore>,
    stateFetcher: StateFetcher,
    positionState?: PositionState
  ) {
    this.address = address;
    this.program = program;
    this.stateFetcher = stateFetcher;
    if (positionState) {
      this.positionState = positionState;
    }
  }

  public async openPosition(
    accounts: OpenPositionAccounts,
    args: OpenPositionArgs
  ) :Promise<TransactionSignature>{
    const {
      tickLowerIndex,
      tickUpperIndex,
      wordLowerIndex,
      wordUpperIndex,
      amount0Desired,
      amount1Desired,
      amount0Min,
      amount1Min,
    } = args;
  
   const tx =  await this.program.methods
      .openPosition(
        tickLowerIndex,
        tickUpperIndex,
        wordLowerIndex,
        wordUpperIndex,
        amount0Desired,
        amount1Desired,
        amount0Min,
        amount1Min
      )
      .accounts(accounts)
      .remainingAccounts([])
      .rpc();

    this.positionState = await this.load();
    return tx
  }

  public async closePosition(accounts: ClosePositionAccounts):Promise<TransactionSignature> {
    return await this.program.methods
      .closePosition()
      .accounts(accounts)
      .remainingAccounts([])
      .rpc();
  }

  public async load(): Promise<PositionState> {
    this.positionState = await this.stateFetcher.getPositionState(this.address);
    return this.positionState;
  }

  public async increaseLiquidity(
    accounts: IncreaseLiquidityAccounts,
    args: IncreaseLiquidityArgs
  ):Promise<TransactionSignature> {
    const { amount0Desired, amount1Desired, amount0Min, amount1Min } = args;

    return await this.program.methods
      .increaseLiquidity(amount0Desired, amount1Desired, amount0Min, amount1Min)
      .accounts(accounts)
      .remainingAccounts([])
      .rpc();
  }

  public async decreaseLiquidity(
    accounts: DecreaseLiquidityAccounts,
    args: DecreaseLiquidityArgs
  ):Promise<TransactionSignature> {
    const { liquidity, amount0Min, amount1Min } = args;

    return await this.program.methods
      .decreaseLiquidity(liquidity, amount0Min, amount1Min)
      .accounts(accounts)
      .remainingAccounts([])
      .rpc();
  }

  public async collectFee(accounts: CollectFeeAccounts, args: CollectFeeArgs) :Promise<TransactionSignature>{
    const { amount0Max, amount1Max } = args;

   return await this.program.methods
      .collectFee(amount0Max, amount1Max)
      .accounts(accounts)
      .remainingAccounts([])
      .rpc();
  }

  public async collectRewards(accounts: CollectRewardsAccounts) :Promise<TransactionSignature>{
    const {
      nftOwner,
      nftAccount,
      poolState,
      protocolPosition,
      personalPosition,
      tickLower,
      tickUpper,
      tokenProgram,
    } = accounts;
   return  await this.program.methods
      .collectRewards()
      .accounts({
        nftOwner,
        nftAccount,
        personalPosition,
        poolState,
        protocolPosition,
        tickLower,
        tickUpper,
        tokenProgram,
      })
      .remainingAccounts(accounts.remainings)
      .rpc();
  }
}
