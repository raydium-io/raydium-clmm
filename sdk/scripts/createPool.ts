#!/usr/bin/env ts-node

import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { Context, NodeWallet } from "../base";
import { OBSERVATION_STATE_LEN } from "../states";
import { sendTransaction } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
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

    const [address, ixs] = await AmmInstruction.createPool(
      ctx,
      {
        poolCreator: owner.publicKey,
        ammConfig: new PublicKey(param.ammConfig),
        tokenMint0: new PublicKey(param.tokenMint0),
        tokenMint1: new PublicKey(param.tokenMint1),
        observation: observation.publicKey,
      },
      param.initialPrice
    );

    const tx = await sendTransaction(
      ctx.provider.connection,
      [createObvIx, ixs],
      [owner, observation],
      defaultConfirmOptions
    );
    console.log("createPool tx: ", tx, " account:", address.toBase58());
  }
}

main();
