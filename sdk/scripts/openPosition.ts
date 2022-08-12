#!/usr/bin/env ts-node

import {
  Connection,
  Keypair,
  ComputeBudgetProgram,
  PublicKey,
} from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { StateFetcher } from "../states";
import { sendTransaction } from "../utils";
import {
  getTickWithPriceAndTickspacing,
  LiquidityMath,
  SqrtPriceMath,
} from "../math";
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

    const tickLower = getTickWithPriceAndTickspacing(
      param.priceLower,
      ammPool.poolState.tickSpacing
    );
    const tickUpper = getTickWithPriceAndTickspacing(
      param.priceUpper,
      ammPool.poolState.tickSpacing
    );

    const priceLower = SqrtPriceMath.getSqrtPriceX64FromTick(tickLower);
    console.log("tickLower:", tickLower, "priceLower:", priceLower.toString());

    const priceUpper = SqrtPriceMath.getSqrtPriceX64FromTick(tickUpper);
    console.log(
      "tickUpper:",
      tickUpper,
      "priceUpper:",
      priceUpper.toString()
    );

    const liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
      poolStateData.sqrtPriceX64,
      priceLower,
      priceUpper,
      param.token0Amount,
      param.token1Amount
    );

    const nftMintAKeypair = new Keypair();
    const [address, openIx] = await AmmInstruction.openPosition(
      {
        payer: owner.publicKey,
        positionNftOwner: owner.publicKey,
        positionNftMint: nftMintAKeypair.publicKey,
        token0Account: token0Account,
        token1Account: token1Account,
      },
      ammPool,
      tickLower,
      tickUpper,
      liquidity,
      param.amountSlippage
    );

    const tx = await sendTransaction(
      ctx.provider.connection,
      [additionalComputeBudgetInstruction, openIx],
      [owner, nftMintAKeypair],
      defaultConfirmOptions
    );
    console.log(
      "openPosition tx: ",
      tx,
      "account:",
      address.toBase58(),
      "tickLower:",
      tickLower,
      "tickUpper:",
      tickUpper
    );
  }
}

main();
