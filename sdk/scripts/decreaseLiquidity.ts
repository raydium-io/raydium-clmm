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
  const stateFetcher = new StateFetcher(ctx.program);
  const params = Config["decrease-liquidity"];
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
    const personalPositionData =
      await stateFetcher.getPersonalPositionState(
        new PublicKey(param.positionId)
      );

    const priceLowerX64 = SqrtPriceMath.getSqrtPriceX64FromTick(
      personalPositionData.tickLowerIndex
    );
    console.log(
      "personalPositionData.tickLowerIndex:",
      personalPositionData.tickLowerIndex,
      "priceLowerX64:",
      priceLowerX64.toString(),
      "priceLower:",
      SqrtPriceMath.sqrtPriceX64ToPrice(
        priceLowerX64,
        ammPool.poolState.mintDecimals0,
        ammPool.poolState.mintDecimals1
      ),
      "liquidity:",
      personalPositionData.liquidity.toString()
    );

    const priceUpperX64 = SqrtPriceMath.getSqrtPriceX64FromTick(
      personalPositionData.tickUpperIndex
    );
    console.log(
      "personalPositionData.tickUpperIndex:",
      personalPositionData.tickUpperIndex,
      "priceUpperX64:",
      priceUpperX64.toString(),
      "priceUpper:",
      SqrtPriceMath.sqrtPriceX64ToPrice(
        priceUpperX64,
        ammPool.poolState.mintDecimals0,
        ammPool.poolState.mintDecimals1
      )
    );

    let tickArrayAddresses: PublicKey[] = [];
    let tickArrayLowerAddress = await getTickArrayAddressByTick(
      ctx.program.programId,
      new PublicKey(param.poolId),
      personalPositionData.tickLowerIndex,
      poolStateData.tickSpacing
    );
    tickArrayAddresses.push(tickArrayLowerAddress);

    let tickArrayUpperAddress = await getTickArrayAddressByTick(
      ctx.program.programId,
      new PublicKey(param.poolId),
      personalPositionData.tickUpperIndex,
      poolStateData.tickSpacing
    );

    if (!tickArrayLowerAddress.equals(tickArrayUpperAddress)) {
      tickArrayAddresses.push(tickArrayUpperAddress);
    }

    const tickArraiesBefore = await stateFetcher.getMultipleTickArrayState(
      tickArrayAddresses
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

    const personalPositionDataUpdated =
      await ammPool.stateFetcher.getPersonalPositionState(
        new PublicKey(param.positionId)
      );
    console.log(
      "personalPositionDataUpdated.liquidity:",
      personalPositionDataUpdated.liquidity.toString(),
      "param.liquidity:",
      param.liquidity.toString()
    );
    assert.equal(
      personalPositionDataUpdated.liquidity.toString(),
      personalPositionData.liquidity.sub(param.liquidity).toString()
    );
    assert.isTrue(personalPositionDataUpdated.tokenFeesOwed0.eqn(0));
    assert.isTrue(personalPositionDataUpdated.tokenFeesOwed1.eqn(0));

    const poolStateDataUpdated = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    console.log(
      "after decrease, pool liquidity:",
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
      poolStateData.tickCurrent >= personalPositionData.tickLowerIndex &&
      poolStateData.tickCurrent < personalPositionData.tickUpperIndex
    ) {
      assert.equal(
        poolStateDataUpdated.liquidity.toString(),
        poolStateData.liquidity.sub(param.liquidity).toString()
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
      personalPositionData.tickLowerIndex,
      poolStateData.tickSpacing
    );
    tickOffsets.push(tickLowerOffset);

    let tickUpperOffset = getTickOffsetInArray(
      personalPositionData.tickUpperIndex,
      poolStateData.tickSpacing
    );
    tickOffsets.push(tickUpperOffset);

    for (let i = 0; i < tickArraiesAfter.length; i++) {
      assert.equal(
        tickArraiesAfter[i].ticks[tickOffsets[i]].liquidityGross.toString(),
        tickArraiesBefore[i].ticks[tickOffsets[i]].liquidityGross
          .sub(param.liquidity)
          .toString()
      );
    }
  }
})();
