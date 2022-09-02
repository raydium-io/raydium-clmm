#!/usr/bin/env ts-node

import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { MintLayout } from "@solana/spl-token";
import { Context, NodeWallet } from "../base";
import { OBSERVATION_STATE_LEN } from "../states";
import { sendTransaction, accountExist } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import { StateFetcher } from "../states";
import keypairFile from "./owner-keypair.json";
import { assert } from "chai";

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
  const params = Config["create-pool"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    const observation = new Keypair();
    const createObvIx = SystemProgram.createAccount({
      fromPubkey: owner.publicKey,
      newAccountPubkey: observation.publicKey,
      lamports: await ctx.provider.connection.getMinimumBalanceForRentExemption(
        OBSERVATION_STATE_LEN
      ),
      space: OBSERVATION_STATE_LEN,
      programId: ctx.program.programId,
    });

    let tokenMint0 = new PublicKey(param.tokenMint0);
    let tokenMint1 = new PublicKey(param.tokenMint1);
    let [decimals0, decimals1] = (
      await connection.getMultipleAccountsInfo([tokenMint0, tokenMint1])
    ).map((t) => (t ? MintLayout.decode(t.data).decimals : 0));
    // @ts-ignore
    if ((tokenMint0._bn as BN).gt(tokenMint1._bn as BN)) {
      const tmp = decimals0;
      decimals0 = decimals1;
      decimals1 = tmp;

      const tmpToken = tokenMint0;
      tokenMint0 = tokenMint1;
      tokenMint1 = tmpToken;
    }
    console.log("decimals0:", decimals0, "decimals1:", decimals1);

    const [address, ixs] = await AmmInstruction.createPool(
      ctx,
      {
        poolCreator: owner.publicKey,
        ammConfig: new PublicKey(param.ammConfig),
        tokenMint0: tokenMint0,
        tokenMint1: tokenMint1,
        observation: observation.publicKey,
      },
      param.initialPrice,
      decimals0,
      decimals1
    );
    const isExist = await accountExist(ctx.connection, address);
    if (isExist) {
      console.log("pool exist, account:", address.toBase58());
      continue;
    }

    const tx = await sendTransaction(
      ctx.provider.connection,
      [createObvIx, ixs],
      [owner, observation],
      defaultConfirmOptions
    );
    console.log("createPool tx: ", tx, " account:", address.toBase58());

    const stateFetcher = new StateFetcher(ctx.program);
    const poolData = await stateFetcher.getPoolState(address);
    assert.equal(poolData.mintDecimals0, decimals0);
    assert.equal(poolData.mintDecimals1, decimals1);
    assert.isTrue(poolData.tokenMint0.equals(tokenMint0));
    assert.isTrue(poolData.tokenMint1.equals(tokenMint1));
    assert.equal(poolData.ammConfig.toString(), param.ammConfig);
    assert.isTrue(poolData.liquidity.eqn(0));
    for (const each of poolData.tickArrayBitmap) {
      assert.equal(each.toString(), "0");
    }
  }
})();
