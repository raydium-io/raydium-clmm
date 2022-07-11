import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../anchor/amm_core";
import { IncreaseLiquidityAccounts } from "./increaseLiquidity";

export type OpenPositionAccounts = {
  payer: PublicKey;
  positionNftMint: PublicKey;
  positionNftOwner: PublicKey;
  positionNftAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickLower: PublicKey;
  tickUpper: PublicKey;
  tickBitmapLower: PublicKey;
  tickBitmapUpper: PublicKey;
  tokenAccount0: PublicKey;
  tokenAccount1: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  lastObservation: PublicKey;
  nextObservation: PublicKey;
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
  wordLowerIndex: number;
  wordUpperIndex: number;
  amount0Desired: BN;
  amount1Desired: BN;
  amount0Min: BN;
  amount1Min: BN;
};

export function openPosition(
  program: Program<AmmCore>,
  args: OpenPositionArgs,
  accounts: OpenPositionAccounts,
): Promise<TransactionInstruction> {
  const {
    tickLowerIndex,
    tickUpperIndex,
    wordLowerIndex,
    wordUpperIndex,
    amount0Desired,
    amount1Desired,
    amount0Min,
    amount1Min,
  } = args;

  return program.methods
    .openPosition(
      tickLowerIndex,
      tickUpperIndex,
      wordLowerIndex,
      wordUpperIndex,
      amount0Desired,
      amount1Desired,
      amount0Min,
      amount1Min
    )
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
