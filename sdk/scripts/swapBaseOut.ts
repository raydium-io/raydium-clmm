#!/usr/bin/env ts-node

import {
  Connection,
  Keypair,
  PublicKey,
  Signer,
  TransactionInstruction,
} from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { StateFetcher } from "../states";
import { sendTransaction } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./owner-keypair.json";

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
  const params = Config["swap-base-out"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    const poolStateData = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
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

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [owner];
    
    const { instructions: ixs, signers: signer }  = await AmmInstruction.swapBaseOut(
      {
        payer: owner.publicKey,
      },
      ammPool,
      new PublicKey(param.outputTokenMint),
      param.amountOut,
      param.amountInSlippage,
      param.priceLimit
    );
    instructions.push(...ixs);
    signers.push(...signer);

    const tx = await sendTransaction(
      ctx.provider.connection,
      instructions,
      signers,
      defaultConfirmOptions
    );
    console.log("swapBaseOut tx: ", tx,"\n");
  }
}

main();
