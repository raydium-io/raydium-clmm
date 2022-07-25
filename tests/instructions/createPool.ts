import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";


export type CreatePoolAccounts = {
    poolCreator: PublicKey,
    ammConfig: PublicKey,
    tokenMint0: PublicKey,
    tokenMint1: PublicKey,
    poolState: PublicKey,
    initialFirstObservation: PublicKey,
    tokenVault0: PublicKey,
    tokenVault1: PublicKey,
    tokenProgram: PublicKey,
    systemProgram: PublicKey,
    rent: PublicKey,
};

export function createPool(
    program: Program<AmmCore>,
    initialPriceX64: BN,
    accounts: CreatePoolAccounts
  ): Promise<TransactionInstruction> {
    return  program.methods.createPool(initialPriceX64).accounts(accounts).instruction()
  }