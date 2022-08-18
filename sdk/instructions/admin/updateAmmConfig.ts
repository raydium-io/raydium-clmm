import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function updateAmmConfigInstruction(
  program: Program<AmmV3>,
  params: {
    newOwner: PublicKey;
    tradeFeeRate: number;
    protocolFeeRate: number;
    flag: number;
  },
  accounts: {
    owner: PublicKey;
    ammConfig: PublicKey;
  }
): Promise<TransactionInstruction> {
  const { newOwner, tradeFeeRate, protocolFeeRate, flag } = params;
  return program.methods
    .updateAmmConfig(newOwner, tradeFeeRate, protocolFeeRate, flag)
    .accounts(accounts)
    .instruction();
}
