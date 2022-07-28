import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program, BN } from "@project-serum/anchor";
import { AmmV3 } from "../anchor/amm_v3";

export type DecreaseLiquidityAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  ammConfig: PublicKey;
  poolState: PublicKey;
  protocolPosition: PublicKey;
  personalPosition: PublicKey;
  tickArrayLower: PublicKey;
  tickArrayUpper: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  tokenProgram: PublicKey;
  recipientTokenAccount0: PublicKey;
  recipientTokenAccount1: PublicKey;
};

export type DecreaseLiquidityArgs = {
  liquidity: BN;
  amount0Min: BN;
  amount1Min: BN;
};

export function decreaseLiquidityInstruction(
  program: Program<AmmV3>,
  args: DecreaseLiquidityArgs,
  accounts: DecreaseLiquidityAccounts
): Promise<TransactionInstruction> {
  const { liquidity, amount0Min, amount1Min } = args;

  return program.methods
    .decreaseLiquidity(liquidity, amount0Min, amount1Min)
    .accounts(accounts)
    .remainingAccounts([])
    .instruction();
}
