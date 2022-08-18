#!/usr/bin/env ts-node

import { Connection, Keypair, PublicKey,ComputeBudgetProgram } from "@solana/web3.js";
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
  const params = Config["swap-router-base-in"];

  const startPool = await generateOnePool(
    ctx,
    new PublicKey(params.startPool.poolId),
    stateFetcher
  );
  let remainRouterPools: AmmPool[] = [];
  for (let i = 0; i < params.remainRouterPoolIds.length; i++) {
    const pool = await generateOnePool(
      ctx,
      new PublicKey(params.remainRouterPoolIds[i]),
      stateFetcher
    );
    remainRouterPools.push(pool);
  }
  const ix = await AmmInstruction.swapRouterBaseIn(
    owner.publicKey,
    {
      ammPool: startPool,
      inputTokenMint: new PublicKey(params.startPool.inputTokenMint),
    },
    remainRouterPools,
    params.amountIn,
    params.amountOutSlippage
  );

  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });

  let tx = await sendTransaction(
    ctx.connection,
    [additionalComputeBudgetInstruction,ix],
    [owner],
    defaultConfirmOptions
  );
  console.log("swapRouterBaseIn tx: ", tx);
}

async function generateOnePool(
  ctx: Context,
  poolId: PublicKey,
  stateFetcher: StateFetcher
): Promise<AmmPool> {
  const poolStateData = await stateFetcher.getPoolState(new PublicKey(poolId));
  const ammConfigData = await stateFetcher.getAmmConfig(
    new PublicKey(poolStateData.ammConfig)
  );
  const ammPool = new AmmPool(
    ctx,
    new PublicKey(poolId),
    poolStateData,
    ammConfigData,
    stateFetcher
  );
  await ammPool.loadCache();
  return ammPool;
}

main();
