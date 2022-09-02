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
import { SqrtPriceMath } from "../math";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./owner-keypair.json";
import { assert } from "chai";
import { getTickOffsetInArray, getTickArrayAddressByTick } from "../entities";

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
  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });
  const stateFetcher = new StateFetcher(ctx.program);
  const params = Config["open-position"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    
    const ammPool = new AmmPool(
      ctx,
      new PublicKey(param.poolId),
      stateFetcher
    );

    await ammPool.loadPoolState()
    const poolStateData = ammPool.poolState

    console.log(
      "pool current tick:",
      poolStateData.tickCurrent,
      "sqrtPriceX64:",
      poolStateData.sqrtPriceX64.toString(),
      "price:",
      ammPool.tokenPrice(),
      "liquidity:",
      poolStateData.liquidity.toString()
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

    const priceLowerX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tickLower);
    console.log(
      "tickLower:",
      tickLower,
      "priceLowerX64:",
      priceLowerX64.toString(),
      "priceLower:",
      SqrtPriceMath.sqrtPriceX64ToPrice(
        priceLowerX64,
        poolStateData.mintDecimals0,
        poolStateData.mintDecimals1
      )
    );

    const priceUpperX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tickUpper);
    console.log(
      "tickUpper:",
      tickUpper,
      "priceUpperX64:",
      priceUpperX64.toString(),
      "priceLower:",
      SqrtPriceMath.sqrtPriceX64ToPrice(
        priceUpperX64,
        poolStateData.mintDecimals0,
        poolStateData.mintDecimals1
      )
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
    );

    const poolStateDataUpdated = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    console.log(
      "after open position, pool updated liquidity:",
      poolStateDataUpdated.liquidity.toString(),
      "\n"
    );
    assert.deepEqual(
      poolStateData.tickCurrent,
      poolStateDataUpdated.tickCurrent
    );
    assert.deepEqual(
      poolStateData.sqrtPriceX64,
      poolStateDataUpdated.sqrtPriceX64
    );
    assert.deepEqual(
      poolStateData.protocolFeesToken0,
      poolStateDataUpdated.protocolFeesToken0
    );
    assert.deepEqual(
      poolStateData.protocolFeesToken1,
      poolStateDataUpdated.protocolFeesToken1
    );
    if (
      poolStateData.tickCurrent >= tickLower &&
      poolStateData.tickCurrent < tickUpper
    ) {
      assert.equal(
        poolStateDataUpdated.liquidity.toString(),
        poolStateData.liquidity.add(param.liquidity).toString()
      );
    } else {
      assert.equal(
        poolStateDataUpdated.liquidity.toString(),
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
      console.log("tickArray start index:",tickArraiesAfter[i].startTickIndex)
      if (tickArraiesBefore[i] != undefined) {
        assert.equal(
          tickArraiesAfter[i].ticks[tickOffsets[i]].liquidityGross.toString(),
          tickArraiesBefore[i].ticks[tickOffsets[i]].liquidityGross
            .add(param.liquidity)
            .toString()
        );
      } else {
        assert.equal(
          tickArraiesAfter[i].ticks[tickOffsets[i]].liquidityGross.toString(),
          param.liquidity.toString()
        );
      }
    }
  }
})();
