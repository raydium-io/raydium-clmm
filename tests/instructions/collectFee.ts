import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program ,BN} from "@project-serum/anchor";
import { AmmCore } from "../../target/types/amm_core";
import {DecreaseLiquidityAccounts} from "./decreaseLiquidity";

export type CollectFeeAccounts = {} & DecreaseLiquidityAccounts;

export type CollectFeeArgs = {
  amount0Max: BN;
  amount1Max: BN;
};

export function collectFee(  program: Program<AmmCore>,args: CollectFeeArgs,accounts: CollectFeeAccounts) : Promise<TransactionInstruction>{
    const { amount0Max, amount1Max } = args;

    return program.methods
      .collectFee(amount0Max, amount1Max)
      .accounts(accounts)
      .remainingAccounts([])
      .instruction();
  }