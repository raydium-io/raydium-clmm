import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";

export type CollectRewardsAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickLower: PublicKey;
  tickUpper: PublicKey;
  tokenProgram: PublicKey;
  remainings: AccountMeta[];
};

export function collectRewards(
  program: Program<AmmCore>,
  accounts: CollectRewardsAccounts
): Promise<TransactionInstruction> {
  const {
    nftOwner,
    nftAccount,
    poolState,
    protocolPosition,
    personalPosition,
    tickLower,
    tickUpper,
    tokenProgram,
  } = accounts;

  return program.methods
    .collectRewards()
    .accounts({
      nftOwner,
      nftAccount,
      personalPosition,
      poolState,
      protocolPosition,
      tickLower,
      tickUpper,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
