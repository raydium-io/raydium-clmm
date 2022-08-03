import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function setNewOwnerInstruction(
  program: Program<AmmV3>,
  accounts: {
    owner: PublicKey;
    newOwner: PublicKey;
    ammConfig: PublicKey;
  }
): Promise<TransactionInstruction> {
  return program.methods.setNewOwner().accounts(accounts).instruction();
}
