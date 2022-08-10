import {
    Connection,
    ConfirmOptions,
    PublicKey,
    Keypair,
    Signer,
    ComputeBudgetProgram,
    TransactionInstruction,
    SystemProgram,
    TransactionSignature,
  } from "@solana/web3.js";
  
  import { Context, NodeWallet } from "../base";
  import { sendTransaction } from "../utils";
  import { AmmInstruction } from "../instructions";
  import { programId, admin, localWallet } from "./config";
  
  export const defaultConfirmOptions: ConfirmOptions = {
    preflightCommitment: "processed",
    commitment: "processed",
    skipPreflight: true,
  };
  
  async function main() {
    const payer = localWallet();
    const connection = new Connection(
      "https://api.devnet.solana.com",
      defaultConfirmOptions.commitment
    );
    const ctx = new Context(
      connection,
      NodeWallet.fromSecretKey(payer),
      programId,
      defaultConfirmOptions
    );
  
  
    tx = await sendTransaction(
      ctx.provider.connection,
      [ix],
      [admin],
      defaultConfirmOptions
    );
  }
  
  main();
  