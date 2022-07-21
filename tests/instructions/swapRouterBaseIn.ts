import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";

export type SwapRouterBaseInAccounts = {
  payer: PublicKey;
  ammConfig: PublicKey;
  inputTokenAccount: PublicKey;
  tokenProgram: PublicKey;
  remainings: AccountMeta[];
};

export type SwapRouterBaseInArgs = {
  amountIn: BN;
  amountOutMinimum: BN;
  additionalAccountsPerPool: number[];
};

export function swapRouterBaseIn(
  program: Program<AmmCore>,
  args: SwapRouterBaseInArgs,
  accounts: SwapRouterBaseInAccounts
): Promise<TransactionInstruction> {
  const { amountIn, amountOutMinimum, additionalAccountsPerPool } = args;

  const { payer, ammConfig, inputTokenAccount, tokenProgram } = accounts;

  return program.methods
    .swapRouterBaseIn(amountIn, amountOutMinimum, additionalAccountsPerPool)
    .accounts({
      payer,
      ammConfig,
      inputTokenAccount,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
