import { PublicKey } from "@solana/web3.js";

export type Context = {
    ammConfig: PublicKey
    programId: PublicKey
}