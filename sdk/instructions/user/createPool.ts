import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";


export function createPoolInstruction(
  program: Program<AmmV3>,
  initialPriceX64: BN,
  accounts:  {
    poolCreator: PublicKey;
    ammConfig: PublicKey;
    tokenMint0: PublicKey;
    tokenMint1: PublicKey;
    poolState: PublicKey;
    observationState: PublicKey;
    tokenVault0: PublicKey;
    tokenVault1: PublicKey;
    tokenProgram: PublicKey;
    systemProgram: PublicKey;
    rent: PublicKey;
  }
): Promise<TransactionInstruction> {
  return program.methods
    .createPool(initialPriceX64)
    .accounts(accounts)
    .instruction();
}
