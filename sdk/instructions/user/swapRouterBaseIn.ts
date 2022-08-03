import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function swapRouterBaseInInstruction(
  program: Program<AmmV3>,
  args: {
    amountIn: BN;
    amountOutMinimum: BN;
  },
  accounts:  {
    payer: PublicKey;
    inputTokenAccount: PublicKey;
    tokenProgram: PublicKey;
    remainings: AccountMeta[];
  }
): Promise<TransactionInstruction> {
  const { amountIn, amountOutMinimum } = args;

  const { payer, inputTokenAccount, tokenProgram } = accounts;

  return program.methods
    .swapRouterBaseIn(amountIn, amountOutMinimum)
    .accounts({
      payer,
      inputTokenAccount,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
