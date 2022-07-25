import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../target/types/amm_core";

export type SwapAccounts = {
  payer: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  inputTokenAccount: PublicKey;
  outputTokenAccount: PublicKey;
  inputVault: PublicKey;
  outputVault: PublicKey;
  lastObservation: PublicKey;
  nextObservation: PublicKey;
  tokenProgram: PublicKey;
  remainings: AccountMeta[];
};

export type SwapArgs = {
  amount: BN;
  otherAmountThreshold: BN;
  sqrtPriceLimitX64: BN;
  isBaseInput: boolean;
};

export function swap(
  program: Program<AmmCore>,
  args: SwapArgs,
  accounts: SwapAccounts
): Promise<TransactionInstruction> {
  const { amount, otherAmountThreshold, sqrtPriceLimitX64, isBaseInput } = args;

  const {
    payer,
    ammConfig,
    poolState,
    inputTokenAccount,
    outputTokenAccount,
    inputVault,
    outputVault,
    lastObservation,
    nextObservation,
    tokenProgram,
  } = accounts;

  return program.methods
    .swap(amount, otherAmountThreshold, sqrtPriceLimitX64, isBaseInput)
    .accounts({
      payer,
      ammConfig,
      poolState,
      inputTokenAccount,
      outputTokenAccount,
      inputVault,
      outputVault,
      lastObservation,
      nextObservation,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
