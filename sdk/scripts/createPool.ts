#!/usr/bin/env ts-node

import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID,MintLayout, AccountLayout } from "@solana/spl-token";
import { Context, NodeWallet } from "../base";
import { OBSERVATION_STATE_LEN } from "../states";
import { sendTransaction, accountExist } from "../utils";
import { AmmInstruction } from "../instructions";
import { Config, defaultConfirmOptions } from "./config";
import keypairFile from "./owner-keypair.json";

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

    const tokenMint0 = new PublicKey(param.tokenMint0)
    const tokenMint1 = new PublicKey(param.tokenMint1)
    const [decimals0, decimals1] = (await connection.getMultipleAccountsInfo(
      [tokenMint0, tokenMint1]
      )).map((t)=>t?MintLayout.decode(t.data).decimals:0)
    // if (!token0Data) {
    //   throw new Error("token0Data is null");
    // }

    // const decimals0 = MintLayout.decode(token0Data.data).decimals;

    // const token1Data = await connection.getAccountInfo(
    //   new PublicKey(param.tokenMint1)
    // );
    // if (!token1Data) {
    //   throw new Error("token1Data is null");
    // }
    // const decimals1 = MintLayout.decode(token1Data.data).decimals;
    console.log("decimals0:",decimals0,"decimals1:",decimals1)

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
  }
})();
