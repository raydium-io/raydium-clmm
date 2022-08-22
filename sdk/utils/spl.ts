import { WSOL, Spl } from "@raydium-io/raydium-sdk";
import {
  PublicKey,
} from "@solana/web3.js";

export function isWSOLTokenMint(tokenMint: PublicKey): boolean {
  return tokenMint.toString() == WSOL.mint;
}
