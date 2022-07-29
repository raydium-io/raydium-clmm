import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export type SetRewardEmissionsAccounts = {
  authority: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
};

export type SetRewardEmissionsArgs = {
  rewardIndex: number;
  emissionsPerSecondX64: BN;
};

export function setRewardEmissionsInstruction(
  program: Program<AmmV3>,
  args: SetRewardEmissionsArgs,
  accounts: SetRewardEmissionsAccounts
): Promise<TransactionInstruction> {
  const { rewardIndex, emissionsPerSecondX64 } = args;

  return program.methods
    .setRewardEmissions(rewardIndex, emissionsPerSecondX64)
    .accounts(accounts)
    .instruction();
}
