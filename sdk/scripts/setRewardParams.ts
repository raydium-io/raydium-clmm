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
import keypairFile from "./admin-keypair.json";
import { MathUtil, SqrtPriceMath } from "../math";
import { assert } from "chai";
import Decimal from "decimal.js";

(async () => {
  const admin = Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)));
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
  const params = Config["set-reward-emissions"];
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

    const { instructions: ixs, signers: signer } = await AmmInstruction.setRewardParams(
      ctx,
      admin.publicKey,
      ammPool,
      param.rewardIndex,
      param.emissionsPerSecond,
      param.openTime,
      param.endTime
    );
    instructions.push(...ixs);
    signers.push(...signer);

    let tx = await sendTransaction(
      ctx.connection,
      instructions,
      signers,
      defaultConfirmOptions
    );
    console.log("setRewardEmissions tx: ", tx);

    const poolStateDataUpdated = await stateFetcher.getPoolState(
      new PublicKey(param.poolId)
    );
    const rewardInfo = poolStateDataUpdated.rewardInfos[param.rewardIndex];
    assert.equal(
      rewardInfo.emissionsPerSecondX64.toString(),
      MathUtil.decimalToX64(new Decimal(param.emissionsPerSecond)).toString()
    );
  }
})();
