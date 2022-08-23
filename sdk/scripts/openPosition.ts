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
import { getTickArrayAddress, sendTransaction } from "../utils";
import { getTickWithPriceAndTickspacing, SqrtPriceMath } from "../math";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./owner-keypair.json";
import { assert } from "chai";
import { getTickOffsetInArray, getTickArrayAddressByTick } from "../entities";

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
    console.log(
      "pool current tick:",
      poolStateData.tickCurrent,
      "sqrtPriceX64:",
      poolStateData.sqrtPriceX64.toString(),
      "price:",
      ammPool.tokenPrice()
    );
    const tickLower = ammPool.getRoundingTickWithPrice(param.priceLower);
    const tickUpper = ammPool.getRoundingTickWithPrice(param.priceUpper);
    if (tickLower % poolStateData.tickSpacing != 0) {
      return;
    }

    let tickArrayAddresses: PublicKey[] = [];
    let tickArrayLowerAddress = await getTickArrayAddressByTick(
      ctx.program.programId,
      new PublicKey(param.poolId),
      tickLower,
      poolStateData.tickSpacing
    );
    tickArrayAddresses.push(tickArrayLowerAddress);

    let tickArrayUpperAddress = await getTickArrayAddressByTick(
      ctx.program.programId,
      new PublicKey(param.poolId),
      tickUpper,
      poolStateData.tickSpacing
    );

    console.log(
      "tickArrayLowerAddress:",
      tickArrayLowerAddress.toString(),
      "tickArrayUpperAddress:",
      tickArrayUpperAddress.toString()
    );
    if (!tickArrayLowerAddress.equals(tickArrayUpperAddress)) {
      tickArrayAddresses.push(tickArrayUpperAddress);
    }

    const tickArraiesBefore = await stateFetcher.getMultipleTickArrayState(
      tickArrayAddresses
    );
    // console.log("tickArraiesBefore:",tickArraiesBefore)
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

    const poolUpdatedData = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    console.log(
      "after open position, pool updated liquidity:",
      poolUpdatedData.liquidity.toString()
    );

    if (
      poolStateData.tickCurrent >= tickLower &&
      poolStateData.tickCurrent < tickUpper
    ) {
      assert.equal(
        poolUpdatedData.liquidity.toString(),
        poolStateData.liquidity.add(param.liquidity).toString()
      );
    } else {
      assert.equal(
        poolUpdatedData.liquidity.toString(),
        poolStateData.liquidity.toString()
      );
    }

    const tickArraiesAfter = await stateFetcher.getMultipleTickArrayState(
      tickArrayAddresses
    );
    assert.equal(tickArraiesBefore.length, tickArraiesAfter.length);

    let tickOffsets: number[] = [];
    let tickLowerOffset = getTickOffsetInArray(
      tickLower,
      poolStateData.tickSpacing
    );
    tickOffsets.push(tickLowerOffset);

    let tickUpperOffset = getTickOffsetInArray(
      tickUpper,
      poolStateData.tickSpacing
    );
    tickOffsets.push(tickUpperOffset);

    for (let i = 0; i < tickArraiesAfter.length; i++) {
      assert.equal(
        tickArraiesAfter[i].ticks[tickOffsets[i]].liquidityGross.toString(),
        tickArraiesBefore[i].ticks[tickOffsets[i]].liquidityGross
          .add(param.liquidity)
          .toString()
      );
    }
  }
}

main();
