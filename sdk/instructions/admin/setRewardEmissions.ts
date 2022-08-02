import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function setRewardEmissionsInstruction(
  program: Program<AmmV3>,
  args: {
    rewardIndex: number;
    emissionsPerSecondX64: BN;
  },
  accounts: {
    authority: PublicKey;
    ammConfig: PublicKey;
    poolState: PublicKey;
  }
): Promise<TransactionInstruction> {
  const { rewardIndex, emissionsPerSecondX64 } = args;

  return program.methods
    .setRewardEmissions(rewardIndex, emissionsPerSecondX64)
    .accounts(accounts)
    .instruction();
}
