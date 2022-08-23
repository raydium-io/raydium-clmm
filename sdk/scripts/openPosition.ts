#!/usr/bin/env ts-node

import {
  Connection,
  Keypair,
  ComputeBudgetProgram,
  PublicKey,
  TransactionInstruction,
  Signer,
} from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { StateFetcher } from "../states";
import { sendTransaction } from "../utils";
import { getTickWithPriceAndTickspacing, SqrtPriceMath } from "../math";
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
  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });
  const stateFetcher = new StateFetcher(ctx.program);
  const params = Config["open-position"];
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

    const tickLower = ammPool.getRoundingTickWithPrice(param.priceLower);
    const tickUpper = ammPool.getRoundingTickWithPrice(param.priceUpper);

    let instructions: TransactionInstruction[] = [
      additionalComputeBudgetInstruction,
    ];
    let signers: Signer[] = [owner];

    const priceLower = SqrtPriceMath.getSqrtPriceX64FromTick(tickLower);
    console.log(
      "tickLower:",
      tickLower,
      "priceLowerX64:",
      priceLower.toString(),
      "priceLower:",
      param.priceLower
    );

    const priceUpper = SqrtPriceMath.getSqrtPriceX64FromTick(tickUpper);
    console.log(
      "tickUpper:",
      tickUpper,
      "priceUpperX64:",
      priceUpper.toString(),
      "priceLower:",
      param.priceUpper
    );
    const nftMintAKeypair = new Keypair();
    signers.push(nftMintAKeypair);
    const {
      personalPosition,
      instructions: ixs,
      signers: signer,
    } = await AmmInstruction.openPosition(
      {
        payer: owner.publicKey,
        positionNftOwner: owner.publicKey,
        positionNftMint: nftMintAKeypair.publicKey,
      },
      ammPool,
      tickLower,
      tickUpper,
      param.liquidity,
      param.amountSlippage
    );
    instructions.push(...ixs);
    signers.push(...signer);

    const tx = await sendTransaction(
      ctx.provider.connection,
      instructions,
      signers,
      defaultConfirmOptions
    );
    console.log(
      "openPosition tx: ",
      tx,
      "account:",
      personalPosition.toBase58(),
      "\n"
    );
  }
}

main();
