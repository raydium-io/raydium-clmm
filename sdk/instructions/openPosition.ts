import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type OpenPositionAccounts = {
  payer: PublicKey;
  positionNftMint: PublicKey;
  positionNftOwner: PublicKey;
  positionNftAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickArrayLower: PublicKey;
  tickArrayUpper: PublicKey;
  tokenAccount0: PublicKey;
  tokenAccount1: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  metadataAccount: PublicKey;
  rent: PublicKey;
  systemProgram: PublicKey;
  tokenProgram: PublicKey;
  associatedTokenProgram: PublicKey;
  metadataProgram: PublicKey;
};

export type OpenPositionArgs = {
  tickLowerIndex: number;
  tickUpperIndex: number;
  tickArrayLowerStartIndex: number;
  tickArrayUpperStartIndex: number;
  amount0Desired: BN;
  amount1Desired: BN;
  amount0Min: BN;
  amount1Min: BN;
};

export function openPositionInstruction(
  program: Program<AmmV3>,
  args: OpenPositionArgs,
  accounts: OpenPositionAccounts,
): Promise<TransactionInstruction> {
  const {
    tickLowerIndex,
    tickUpperIndex,
    tickArrayLowerStartIndex,
    tickArrayUpperStartIndex,
    amount0Desired,
    amount1Desired,
    amount0Min,
    amount1Min,
  } = args;

  return program.methods
    .openPosition(
      tickLowerIndex,
      tickUpperIndex,
      tickArrayLowerStartIndex,
      tickArrayUpperStartIndex,
      amount0Desired,
      amount1Desired,
      amount0Min,
      amount1Min
    )
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
