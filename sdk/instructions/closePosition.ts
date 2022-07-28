import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type ClosePositionAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  positionNftMint: PublicKey;
  personalPosition: PublicKey;
  tokenProgram: PublicKey;
  systemProgram: PublicKey;
};

export function closePositionInstruction(
  program: Program<AmmV3>,
  accounts: ClosePositionAccounts
): Promise<TransactionInstruction> {
  return program.methods
    .closePosition()
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
