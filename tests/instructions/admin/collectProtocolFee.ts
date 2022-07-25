import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../../target/types/amm_core";

export type CollectProtocolFeeAccounts = {
  owner: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  recipientTokenAccount0: PublicKey;
  recipientTokenAccount1: PublicKey;
  tokenProgram: PublicKey;
};

export type CollectProtocolFeeArgs = {
  amount0Requested: BN;
  amount1Requested: BN;
};

export function collectProtocolFee(
  program: Program<AmmCore>,
  args: CollectProtocolFeeArgs,
  accounts: CollectProtocolFeeAccounts

): Promise<TransactionInstruction> {
  const { amount0Requested, amount1Requested } = args;

  return program.methods
    .collectProtocolFee(amount0Requested, amount1Requested)
    .accounts(accounts)
    .instruction();
}
