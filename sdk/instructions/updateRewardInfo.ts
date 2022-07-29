import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type UpdateRewardInfosAccounts = {
  ammConfig: PublicKey;
  poolState: PublicKey;
};

export function updateRewardInfosInstruction(
  program: Program<AmmV3>,
  accounts: UpdateRewardInfosAccounts
): Promise<TransactionInstruction> {
  return program.methods.updateRewardInfos().accounts(accounts).instruction();
}
