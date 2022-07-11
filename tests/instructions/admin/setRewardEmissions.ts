import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../anchor/amm_core";

export type SetRewardEmissionsAccounts = {
  authority: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
};

export type SetRewardEmissionsArgs = {
  rewardIndex: number;
  emissionsPerSecondX64: BN;
};

export function setRewardEmissions(
  program: Program<AmmCore>,
  args: SetRewardEmissionsArgs,
  accounts: SetRewardEmissionsAccounts
): Promise<TransactionInstruction> {
  const { rewardIndex, emissionsPerSecondX64 } = args;

  return program.methods
    .setRewardEmissions(rewardIndex, emissionsPerSecondX64)
    .accounts(accounts)
    .instruction();
}
