import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function updateRewardInfosInstruction(
  program: Program<AmmV3>,
  accounts: {
    poolState: PublicKey;
  }
): Promise<TransactionInstruction> {
  return program.methods.updateRewardInfos().accounts(accounts).instruction();
}
