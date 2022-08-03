import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function setProtocolFeeRateInstruction(
  program: Program<AmmV3>,
  protocolFeeRate: number,
  accounts: {
    owner: PublicKey;
    ammConfig: PublicKey;
    systemprogram: PublicKey;
  }
): Promise<TransactionInstruction> {
  return program.methods
    .setProtocolFeeRate(protocolFeeRate)
    .accounts(accounts)
    .instruction();
}
