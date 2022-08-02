import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";


export function swapInstruction(
  program: Program<AmmV3>,
  args: {
    amount: BN;
    otherAmountThreshold: BN;
    sqrtPriceLimitX64: BN;
    isBaseInput: boolean;
  },
  accounts:  {
    payer: PublicKey;
    ammConfig: PublicKey;
    poolState: PublicKey;
    inputTokenAccount: PublicKey;
    outputTokenAccount: PublicKey;
    inputVault: PublicKey;
    outputVault: PublicKey;
    tickArray: PublicKey;
    observationState: PublicKey;
    tokenProgram: PublicKey;
    remainings: AccountMeta[];
  }
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
    tickArray,
    observationState,
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
      tickArray,
      observationState,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
