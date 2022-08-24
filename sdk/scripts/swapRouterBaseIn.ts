#!/usr/bin/env ts-node

import {
  Connection,
  Keypair,
  PublicKey,
  ComputeBudgetProgram,
  TransactionInstruction,
  Signer,
} from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { StateFetcher } from "../states";
import { sendTransaction } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./owner-keypair.json";


(async () => {
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

  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });
  let instructions: TransactionInstruction[] = [
    additionalComputeBudgetInstruction,
  ];
  let signers: Signer[] = [owner];

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
  const { instructions: ixs, signers: signer } =
    await AmmInstruction.swapRouterBaseIn(
      owner.publicKey,
      {
        ammPool: startPool,
        inputTokenMint: new PublicKey(params.startPool.inputTokenMint),
      },
      remainRouterPools,
      params.amountIn,
      params.amountOutSlippage
    );
  instructions.push(...ixs);
  signers.push(...signer);

  let tx = await sendTransaction(
    ctx.connection,
    instructions,
    signers,
    defaultConfirmOptions
  );
  console.log("swapRouterBaseIn tx: ", tx);
})()

async function generateOnePool(
  ctx: Context,
  poolId: PublicKey,
  stateFetcher: StateFetcher
): Promise<AmmPool> {

  const ammPool = new AmmPool(
    ctx,
    new PublicKey(poolId),
    stateFetcher
  );
  await ammPool.loadPoolState()
  return ammPool;
}
