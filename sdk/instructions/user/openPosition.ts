import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";

export function openPositionInstruction(
  program: Program<AmmV3>,
  args: {
    tickLowerIndex: number;
    tickUpperIndex: number;
    tickArrayLowerStartIndex: number;
    tickArrayUpperStartIndex: number;
    liquidity: BN;
    amount0Max: BN;
    amount1Max: BN;
  },
  accounts: {
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
  }
): Promise<TransactionInstruction> {
  const {
    tickLowerIndex,
    tickUpperIndex,
    tickArrayLowerStartIndex,
    tickArrayUpperStartIndex,
    liquidity,
    amount0Max,
    amount1Max,
  } = args;
  return program.methods
    .openPosition(
      tickLowerIndex,
      tickUpperIndex,
      tickArrayLowerStartIndex,
      tickArrayUpperStartIndex,
      liquidity,
      amount0Max,
      amount1Max
    )
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
