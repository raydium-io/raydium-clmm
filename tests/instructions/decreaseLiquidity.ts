import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmCore } from "../../target/types/amm_core";

export type DecreaseLiquidityAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickLower: PublicKey;
  tickUpper: PublicKey;
  tickBitmapLower: PublicKey;
  tickBitmapUpper: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  lastObservation: PublicKey;
  nextObservation: PublicKey;
  tokenProgram: PublicKey;
  recipientTokenAccount_0: PublicKey;
  recipientTokenAccount_1: PublicKey;
};

export type DecreaseLiquidityArgs = {
  liquidity: BN;
  amount0Min: BN;
  amount1Min: BN;
};

export function decreaseLiquidity(
  program: Program<AmmCore>,
  args: DecreaseLiquidityArgs,
  accounts: DecreaseLiquidityAccounts
): Promise<TransactionInstruction> {
  const { liquidity, amount0Min, amount1Min } = args;

 return  program.methods
    .decreaseLiquidity(liquidity, amount0Min, amount1Min)
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
