import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export type InitializeRewardAccounts = {
  rewardFunder: PublicKey;
  funderTokenAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  rewardTokenMint: PublicKey;
  rewardTokenVault: PublicKey;
  tokenProgram: PublicKey;
  systemProgram: PublicKey;
  rent: PublicKey;
};

export type InitializeRewardArgs = {
  rewardIndex: number;
  openTime: BN;
  endTime: BN;
  emissionsPerSecondX64: BN;
};

export function initializeRewardInstruction(
  program: Program<AmmV3>,
  args: InitializeRewardArgs,
  accounts: InitializeRewardAccounts
): Promise<TransactionInstruction> {
  return program.methods
    .initializeReward(args)
    .accounts(accounts)
    .instruction();
}
