#!/usr/bin/env ts-node

import {
  Connection,
  PublicKey,
  Keypair,
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
import { SqrtPriceMath } from "../math";
import { Spl } from "@raydium-io/raydium-sdk";
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
  const params = Config["decrease-liquidity"];
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

    const personalPositionData =
      await ammPool.stateFetcher.getPersonalPositionState(
        new PublicKey(param.positionId)
      );
    console.log(
      "personalPositionData.tickLowerIndex:",
      personalPositionData.tickLowerIndex,
      "priceLower:",
      SqrtPriceMath.getSqrtPriceX64FromTick(
        personalPositionData.tickLowerIndex
      ).toString()
    );

    console.log(
      "personalPositionData.tickUpperIndex:",
      personalPositionData.tickUpperIndex,
      "priceUpper:",
      SqrtPriceMath.getSqrtPriceX64FromTick(
        personalPositionData.tickUpperIndex
      ).toString()
    );

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [owner];
    const { instructions: ixs, signers: signer } =
      await AmmInstruction.decreaseLiquidity(
        {
          positionNftOwner: owner.publicKey,
        },
        ammPool,
        personalPositionData,
        param.liquidity,
        param.amountSlippage
      );
    instructions.push(...ixs);
    signers.push(...signer);

    let tx = await sendTransaction(
      ctx.connection,
      instructions,
      signers,
      defaultConfirmOptions
    );
    console.log("decreaseLiquidity tx: ", tx);
  }
}

main();
