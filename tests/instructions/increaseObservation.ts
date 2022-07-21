import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";

export type IncreaseObservationAccounts = {
  payer: PublicKey;
  poolState: PublicKey;
  systemProgram: PublicKey;
  remainings: AccountMeta[];
};

export function increaseObservation(
  program: Program<AmmCore>,
  observationAccountBumps: number[],
  accounts: IncreaseObservationAccounts
): Promise<TransactionInstruction> {
  const { payer, poolState, systemProgram } = accounts;
  return program.methods
    .increaseObservation(observationAccountBumps)
    .accounts({
      payer,
      poolState,
      systemProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
