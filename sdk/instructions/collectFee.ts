import {TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";
import { DecreaseLiquidityAccounts } from "./decreaseLiquidity";

export type CollectFeeAccounts = {} & DecreaseLiquidityAccounts;

export type CollectFeeArgs = {
  amount0Max: BN;
  amount1Max: BN;
};

export function collectFeeInstruction(
  program: Program<AmmV3>,
  args: CollectFeeArgs,
  accounts: CollectFeeAccounts
): Promise<TransactionInstruction> {
  const { amount0Max, amount1Max } = args;
  return program.methods
    .collectFee(amount0Max, amount1Max)
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
