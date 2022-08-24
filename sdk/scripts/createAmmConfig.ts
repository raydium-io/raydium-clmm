#!/usr/bin/env ts-node
import { Connection, Keypair } from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { sendTransaction, accountExist } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import keypairFile from "./admin-keypair.json";

(async () => {
  const admin = Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)));
  console.log("admin:",admin.publicKey.toString())
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

  const params = Config["create-amm-config"];
  for (let i = 0; i < params.length; i++) {
    const param = params[i];
    let [address, ix] = await AmmInstruction.createAmmConfig(
      ctx,
      admin.publicKey,
      param.index,
      param.tickSpacing,
      param.tradeFeeRate,
      param.protocolFeeRate
    );
    if (await accountExist(connection, address)) {
      console.log(
        "amm config account already exist, address:",
        address.toString()
      );
    } else {
      const tx = await sendTransaction(
        ctx.provider.connection,
        [ix],
        [admin],
        defaultConfirmOptions
      );

      console.log(
        "init amm config tx: ",
        tx,
        "account:",
        address.toString(),
        "index:",
        param.index,
        "tickSpacing:",
        param.tickSpacing,
        "tradeFeeRate:",
        param.tradeFeeRate,
        "/1000000",
        "protocolFeeRate:",
        param.protocolFeeRate,
        "/1000000"
      );
    }
  }
})();
