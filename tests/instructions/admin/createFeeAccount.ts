import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../anchor/amm_core";

export type CreateFeeAccounts = {
  owner: PublicKey;
  ammConfig: PublicKey;
  systemprogram: PublicKey;
};

export type CreateFeeArgs = {
  fee: number;
  tickSpacing: number;
};

export function createFee(
  program: Program<AmmCore>,
  args: CreateFeeArgs,
  accounts: CreateFeeAccounts
): Promise<TransactionInstruction> {
  const { fee, tickSpacing } = args;

  return program.methods
    .createFeeAccount(fee, tickSpacing)
    .accounts(accounts)
    .instruction();
}
