import {
  AccountMeta,
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function setRewardParamsInstruction(
  program: Program<AmmV3>,
  args: {
    rewardIndex: number;
    emissionsPerSecondX64: BN;
    openTimestamp: BN;
    endTimestamp: BN;
  },
  accounts: {
    authority: PublicKey;
    ammConfig: PublicKey;
    poolState: PublicKey;
  },
  remainings: AccountMeta[]
): Promise<TransactionInstruction> {
  const { rewardIndex, emissionsPerSecondX64, openTimestamp, endTimestamp } =
    args;

  return program.methods
    .setRewardParams(
      rewardIndex,
      emissionsPerSecondX64,
      openTimestamp,
      endTimestamp
    )
    .accounts(accounts)
    .remainingAccounts(remainings)
    .instruction();
}
