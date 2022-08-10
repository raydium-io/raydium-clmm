import {
  Connection,
  ConfirmOptions,
  PublicKey,
  Keypair,
  Signer,
  ComputeBudgetProgram,
  TransactionInstruction,
  SystemProgram,
  TransactionSignature,
} from "@solana/web3.js";

import { Context, NodeWallet } from "../base";
import { sendTransaction, accountExist } from "../utils";
import { AmmInstruction } from "../instructions";
import { programId, admin, localWallet, url } from "./config";

export const defaultConfirmOptions: ConfirmOptions = {
  preflightCommitment: "processed",
  commitment: "processed",
  skipPreflight: true,
};

async function main() {
  const payer = localWallet();
  const connection = new Connection(url, defaultConfirmOptions.commitment);

  const ctx = new Context(
    connection,
    NodeWallet.fromSecretKey(payer),
    programId,
    defaultConfirmOptions
  );

  let [index, tickSpacing, tradeFeeRate, protocolFeeRate] = [0, 10, 100, 12000];

  let [address, ix] = await AmmInstruction.createAmmConfig(
    ctx,
    admin.publicKey,
    index,
    tickSpacing,
    tradeFeeRate,
    protocolFeeRate
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
      index,
      "tickSpacing:",
      tickSpacing,
      "tradeFeeRate:",
      tradeFeeRate,
      "/1000000",
      "protocolFeeRate:",
      protocolFeeRate,
      "/1000000"
    );
  }
  [index, tickSpacing, tradeFeeRate, protocolFeeRate] = [1, 60, 2500, 12000];

  [address, ix] = await AmmInstruction.createAmmConfig(
    ctx,
    admin.publicKey,
    index,
    tickSpacing,
    tradeFeeRate,
    protocolFeeRate
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
      index,
      "tickSpacing:",
      tickSpacing,
      "tradeFeeRate:",
      tradeFeeRate,
      "/1000000",
      "protocolFeeRate:",
      protocolFeeRate,
      "/1000000"
    );
  }
}

main();
