import {
  PublicKey,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type CollectRewardsAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickArrayLower: PublicKey;
  tickArrayUpper: PublicKey;
  tokenProgram: PublicKey;
  remainings: AccountMeta[];
};

export function collectRewardsInstruction(
  program: Program<AmmV3>,
  accounts: CollectRewardsAccounts
): Promise<TransactionInstruction> {
  const {
    nftOwner,
    nftAccount,
    poolState,
    protocolPosition,
    personalPosition,
    tickArrayLower,
    tickArrayUpper,
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
      tickArrayLower,
      tickArrayUpper,
      tokenProgram,
    })
    .remainingAccounts(accounts.remainings)
    .instruction();
}
