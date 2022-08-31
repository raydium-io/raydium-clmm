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
import { sendTransaction, getBlockTimestamp } from "../utils";
import { AmmInstruction } from "../instructions";
import { fetchAllPositionsByOwner } from "../position";
import { Config, defaultConfirmOptions } from "./config";
import { AmmPool } from "../pool";
import keypairFile from "./admin-keypair.json";
import { MathUtil, SqrtPriceMath } from "../math";
import { assert } from "chai";
import { getTickOffsetInArray, getTickArrayAddressByTick } from "../entities";
import Decimal from "decimal.js";
import { BN } from "@project-serum/anchor";

(async () => {
  const admin = Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)));
  console.log("admin:", admin.publicKey.toBase58());
  const connection = new Connection(
    Config.url,
    defaultConfirmOptions.commitment
  );
  const ctx = new Context(
    connection,
    NodeWallet.fromSecretKey(admin),
    Config.programId,
    defaultConfirmOptions
  );
  const stateFetcher = new StateFetcher(ctx.program);
  const params = Config["initialize-reward"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];

    const ammPool = new AmmPool(ctx, new PublicKey(param.poolId), stateFetcher);
    await ammPool.loadPoolState();
    const poolStateData = ammPool.poolState;
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

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [admin];

    let openTime = param.openTime;
    let endTime = param.endTime;
    // only for test
    if (openTime.eqn(0)) {
      const current = await getBlockTimestamp(ctx.connection);
      openTime = new BN(current + 3);
    }
    if (endTime.eqn(0)) {
      endTime = openTime.addn(10);
    }
    console.log(
      "init param, openTime:",
      openTime.toString(),
      "endTime:",
      endTime.toString()
    );
    const { instructions: ixs, signers: signer } =
      await AmmInstruction.initializeReward(
        ctx,
        admin.publicKey,
        ammPool,
        new PublicKey(param.rewardTokenMint),
        param.rewardIndex,
        openTime,
        endTime,
        param.emissionsPerSecond
      );
    instructions.push(...ixs);
    signers.push(...signer);

    let tx = await sendTransaction(
      ctx.connection,
      instructions,
      signers,
      defaultConfirmOptions
    );
    console.log("initializeReward tx: ", tx);

    const poolStateDataUpdated = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    const rewardInfo = poolStateDataUpdated.rewardInfos[param.rewardIndex];
    assert.equal(rewardInfo.openTime.toString(), openTime.toString());
    assert.equal(rewardInfo.endTime.toString(), endTime.toString());
    assert.equal(
      rewardInfo.emissionsPerSecondX64.toString(),
      MathUtil.decimalToX64(new Decimal(param.emissionsPerSecond)).toString()
    );
    assert.equal(rewardInfo.rewardClaimed.toString(), "0");
    assert.equal(rewardInfo.rewardTotalEmissioned.toString(), "0");
    assert.isTrue(
      rewardInfo.tokenMint.equals(new PublicKey(param.rewardTokenMint))
    );
  }
})();
