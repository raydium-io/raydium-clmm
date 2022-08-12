import {
  PublicKey,
  Signer,
  TransactionSignature,
  TokenAccountsFilter,
} from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { SPL_ACCOUNT_LAYOUT, SPL_MINT_LAYOUT } from "@raydium-io/raydium-sdk";
import { PositionState, StateFetcher } from "../states";
import {
  OpenPositionAccounts,
  DecreaseLiquidityAccounts,
  ClosePositionAccounts,
  AmmInstruction,
} from "../instructions";
import { AmmPool } from "../pool";
import { BN } from "@project-serum/anchor";
import { getPersonalPositionAddress, sendTransaction } from "../utils";
import { Context } from "../base";

export type MultiplePosition = {
  pubkey: PublicKey;
  state: PositionState;
};

export class Position {
  public readonly ctx: Context;
  public readonly stateFetcher: StateFetcher;

  public address: PublicKey;
  public positionState: PositionState;

  public constructor(
    ctx: Context,
    address: PublicKey,
    stateFetcher: StateFetcher,
    positionState?: PositionState
  ) {
    this.address = address;
    this.ctx = ctx;
    this.stateFetcher = stateFetcher;
    if (positionState) {
      this.positionState = positionState;
    }
  }

  public static async fetchAllPositionsByOwner(
    ctx: Context,
    owner: PublicKey,
    stateFetcher: StateFetcher
  ): Promise<
    {
      pubkey: PublicKey;
      state: PositionState;
    }[]
  > {
    const filter: TokenAccountsFilter = { programId: TOKEN_PROGRAM_ID };
    const result = await ctx.connection.getTokenAccountsByOwner(owner, filter);

    let allPositions: {
      pubkey: PublicKey;
      state: PositionState;
    }[] = [];

    let allMints: PublicKey[] = [];
    for (let i = 0; i < result.value.length; i++) {
      const { mint } = SPL_ACCOUNT_LAYOUT.decode(result.value[i].account.data);
      allMints.push(mint);
    }
    let fetchCount = Math.floor(allMints.length / 100);
    if (allMints.length % 100 != 0) {
      fetchCount += 1;
    }

    for (let i = 0; i < fetchCount; i++) {
      const start = i * 100;
      let end = start + 100;
      if (end > allMints.length) {
        end = allMints.length;
      }
      const mints = allMints.slice(start, end);
      let positionAddresses: PublicKey[] = [];
      const mintAccountInfos = await ctx.connection.getMultipleAccountsInfo(
        mints
      );
      for (let i = 0; i < mintAccountInfos.length; i++) {
        const info = mintAccountInfos[i];
        if (info != null) {
          const { supply, decimals } = SPL_MINT_LAYOUT.decode(info.data);
          if (supply.eqn(1) && decimals == 0) {
            const [positionAddress] = await getPersonalPositionAddress(
              mints[i],
              ctx.program.programId
            );
            positionAddresses.push(positionAddress);
          }
        }
      }

      const states = await stateFetcher.getMultiplePersonalPositionStates(
        positionAddresses
      );
      for (let i = 0; i < states.length; i++) {
        const state = states[i];
        if (state != null) {
          allPositions.push({ pubkey: positionAddresses[i], state });
        }
      }
    }
    return allPositions;
  }

  /**
   *
   * @param signer
   * @param accounts
   * @param ammPool
   * @param tickLowerIndex
   * @param tickUpperIndex
   * @param liquidity
   * @param amountSlippage
   * @returns
   */
  public async openPosition(
    signer: Signer,
    accounts: OpenPositionAccounts,
    ammPool: AmmPool,
    tickLowerIndex: number,
    tickUpperIndex: number,
    liquidity: BN,
    amountSlippage?: number
  ): Promise<TransactionSignature> {
    const [positionAddress, ix] = await AmmInstruction.openPosition(
      accounts,
      ammPool,
      tickLowerIndex,
      tickUpperIndex,
      liquidity,
      amountSlippage
    );
    this.address = positionAddress;
    const tx = await sendTransaction(this.ctx.connection, [ix], [signer]);
    this.positionState = await this.stateFetcher.getPersonalPositionState(
      this.address
    );
    return tx;
  }

  /**
   *
   * @param signer
   * @param accounts
   * @param ammPool
   * @returns
   */
  public async closePosition(
    signer: Signer,
    accounts: ClosePositionAccounts,
    ammPool: AmmPool
  ): Promise<TransactionSignature> {
    const ix = await AmmInstruction.closePosition(accounts, ammPool);
    return await sendTransaction(this.ctx.connection, [ix], [signer]);
  }

  /**
   *
   * @param signer
   * @param accounts
   * @param ammPool
   * @returns
   */
  public async collectFeeAndRewards(
    signer: Signer,
    accounts: DecreaseLiquidityAccounts,
    ammPool: AmmPool
  ): Promise<TransactionSignature> {
    const ix = await AmmInstruction.decreaseLiquidity(
      accounts,
      ammPool,
      this.positionState,
      new BN(0)
    );
    return await sendTransaction(this.ctx.connection, [ix], [signer]);
  }
}
