import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../../target/types/amm_core";

export type ResetSqrtPriceAccounts = {
  owner: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
};

export function resetSqrtPrice(
  program: Program<AmmCore>,
  sqrt_price_x64: BN,
  accounts: ResetSqrtPriceAccounts
): Promise<TransactionInstruction> {
  return program.methods
    .increaseObservation(sqrt_price_x64)
    .accounts(accounts)
    .instruction();
}
