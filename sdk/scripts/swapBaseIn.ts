#!/usr/bin/env ts-node

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { StateFetcher } from "../states";
import { sendTransaction } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./owner-keypair.json";
import {
  Token,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

async function main() {
  const owner = Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)));
  const connection = new Connection(
    Config.url,
    defaultConfirmOptions.commitment
  );
  const ctx = new Context(
    connection,
    NodeWallet.fromSecretKey(owner),
    Config.programId,
    defaultConfirmOptions
  );
  const stateFetcher = new StateFetcher(ctx.program);
  const params = Config["swap-base-in"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    const poolStateData = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    const token0Account = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      poolStateData.tokenMint0,
      owner.publicKey
    );
    const token1Account = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      poolStateData.tokenMint1,
      owner.publicKey
    );
    const ammConfigData = await stateFetcher.getAmmConfig(
      new PublicKey(poolStateData.ammConfig)
    );
    const ammPool = new AmmPool(
      ctx,
      new PublicKey(param.poolId),
      poolStateData,
      ammConfigData,
      stateFetcher
    );
    await ammPool.loadCache();
    
    let inputTokenAccount = token0Account
    let outputTokenAccount = token1Account
    if (new PublicKey(param.inputTokenMint).equals(poolStateData.tokenMint1)){
      inputTokenAccount = token1Account
      outputTokenAccount = token0Account
    }
    const ix = await AmmInstruction.swapBaseIn(
      {
        payer: owner.publicKey,
        inputTokenAccount,
        outputTokenAccount,
      },
      ammPool,
      new PublicKey(param.inputTokenMint),
      param.amountIn,
      param.amountOutSlippage,
      param.priceLimit
    );
    let tx = await sendTransaction(
      ctx.connection,
      [ix],
      [owner],
      defaultConfirmOptions
    );
    console.log("swapBaseIn tx: ", tx);
  }
}

main();
