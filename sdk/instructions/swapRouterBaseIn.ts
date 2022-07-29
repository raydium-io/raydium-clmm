import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type SwapRouterBaseInAccounts = {
  payer: PublicKey;
  inputTokenAccount: PublicKey;
  tokenProgram: PublicKey;
  remainings: AccountMeta[];
};

export type SwapRouterBaseInArgs = {
  amountIn: BN;
  amountOutMinimum: BN;
  additionalAccountsPerPool: Buffer;
};

export function swapRouterBaseInInstruction(
  program: Program<AmmV3>,
  args: SwapRouterBaseInArgs,
  accounts: SwapRouterBaseInAccounts
): Promise<TransactionInstruction> {
  const { amountIn, amountOutMinimum, additionalAccountsPerPool } = args;

  const { payer, inputTokenAccount, tokenProgram } = accounts;

  return program.methods
    .swapRouterBaseIn(amountIn, amountOutMinimum, additionalAccountsPerPool)
    .accounts({
      payer,
      inputTokenAccount,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
