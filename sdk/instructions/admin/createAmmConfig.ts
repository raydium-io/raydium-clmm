import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function createAmmConfigInstruction(
  program: Program<AmmV3>,
  args: {
    index: number;
    tickSpacing: number;
    globalFeeRate: number;
    protocolFeeRate: number;
  },
  accounts: {
    owner: PublicKey;
    ammConfig: PublicKey;
    systemProgram: PublicKey;
  }
): Promise<TransactionInstruction> {
  const { index, tickSpacing, globalFeeRate, protocolFeeRate } = args;
  return program.methods
    .createAmmConfig(index, tickSpacing, globalFeeRate, protocolFeeRate)
    .accounts(accounts)
    .instruction();
}
