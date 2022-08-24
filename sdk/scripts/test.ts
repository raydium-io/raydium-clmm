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
import { Config, defaultConfirmOptions } from "./config";
import keypairFile from "./owner-keypair.json";
import { SqrtPriceMath } from "../math";
import { fetchAllPositionsByOwner } from "../position";
import { bs58 } from "@project-serum/anchor/dist/cjs/utils/bytes";


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
    const positions = await fetchAllPositionsByOwner(ctx, owner.publicKey, stateFetcher)

    const accounts:PublicKey[] = []
    for(const {state} of positions){
        accounts.push(state.poolId)
    }
    const poolStatas = await stateFetcher.getMultiplePoolStates(accounts)

    for(const [i,{pubkey, state}] of positions.entries()){
        // const priceUpperX64 = SqrtPriceMath.getSqrtPriceX64FromTick(state.tickUpperIndex);
        // SqrtPriceMath.sqrtPriceX64ToPrice(SqrtPriceMath.getSqrtPriceX64FromTick(state.tickUpperIndex),0,0)
        console.log(`${pubkey.toBase58()}, ${state.poolId.toBase58()}, 
        ${state.tickLowerIndex}, ${SqrtPriceMath.sqrtPriceX64ToPrice(SqrtPriceMath.getSqrtPriceX64FromTick(state.tickLowerIndex),poolStatas[i].mint0Decimals,poolStatas[i].mint1Decimals)}, 
        ${state.tickUpperIndex}, ${SqrtPriceMath.sqrtPriceX64ToPrice(SqrtPriceMath.getSqrtPriceX64FromTick(state.tickUpperIndex),poolStatas[i].mint0Decimals,poolStatas[i].mint1Decimals)}, 
        ${state.liquidity}`)
    }

})()

  