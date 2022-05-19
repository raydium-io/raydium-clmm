// Airdrop SOL and tokens to any wallet using
// `ts-node 4-airdrop-to-wallet 4c3G7V1DNx8LHQ5bwM7okugdxB1Q1caXmtbfDCZfXBLH`

import * as anchor from '@project-serum/anchor'
import { Keypair, Transaction, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { Program, web3 } from '@project-serum/anchor'
import keypairFile from './keypair.json'
import { CyclosCore } from '../target/types/cyclos_core';
import { ASSOCIATED_TOKEN_PROGRAM_ID, Token, TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { FEE_SEED, OBSERVATION_SEED, POOL_SEED, u16ToSeed, u32ToSeed } from '@cykura/sdk';

export const FAUCET_AUTHORITY = Keypair.fromSecretKey(
  Uint8Array.from([166, 35, 198, 106, 198, 244, 143, 224, 64, 125, 232, 144, 28, 45, 178, 146, 56, 92, 99, 244, 25, 75, 104, 247, 215, 33, 62, 30, 186, 249, 163, 48, 185, 210, 115, 123, 192, 235, 130, 28, 35, 27, 9, 65, 38, 210, 100, 190, 62, 225, 55, 90, 209, 0, 227, 160, 141, 54, 132, 242, 98, 240, 212, 95])
);
const usdcMint = new web3.PublicKey('GyH7fsFCvD1Wt8DbUGEk6Hzt68SVqwRKDHSvyBS16ZHm')
const usdtMint = new web3.PublicKey('7HvgZSj1VqsGADkpb8jLXCVqyzniDHP5HzQCymHnrn1t')
const wsolMint = new web3.PublicKey('EC1x3JZ1PBW4MqH711rqfERaign6cxLTBNb3mi5LK9vP')

async function main() {

  const keypair = web3.Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)))
  console.log('pubkey', keypair.publicKey.toString())
  const wallet = new anchor.Wallet(keypair)
  const owner = wallet.publicKey
  const connection = new web3.Connection('http://127.0.0.1:8899')
  const provider = new anchor.Provider(connection, wallet, {})
  anchor.setProvider(provider)
  const coreProgram = anchor.workspace.CyclosCore as Program<CyclosCore>

  const fee = 500
  const [poolAState, poolAStateBump] = await web3.PublicKey.findProgramAddress(
    [
      POOL_SEED,
      usdtMint.toBuffer(),
      usdcMint.toBuffer(),
      u32ToSeed(fee)
    ],
    coreProgram.programId
  )

  const [feeState, feeStateBump] = await web3.PublicKey.findProgramAddress(
    [FEE_SEED, u32ToSeed(fee)],
    coreProgram.programId
  );

  const [initialObservationStateA, initialObservationBumpA] = await web3.PublicKey.findProgramAddress(
    [
      OBSERVATION_SEED,
      usdtMint.toBuffer(),
      usdcMint.toBuffer(),
      u32ToSeed(fee),
      u16ToSeed(0)
    ],
    coreProgram.programId
  );

  const vaultA0 = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    usdtMint,
    poolAState,
    true
  )
  const vaultA1 = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    usdcMint,
    poolAState,
    true
  )

  const tx = coreProgram.transaction.createAndInitPool(new anchor.BN(4294967296), {
    accounts: {
      poolCreator: owner,
      token0: usdtMint,
      token1: usdcMint,
      feeState,
      poolState: poolAState,
      initialObservationState: initialObservationStateA,
      vault0: vaultA0,
      vault1: vaultA1,
      systemProgram: SystemProgram.programId,
      rent: web3.SYSVAR_RENT_PUBKEY,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID
    }
  })

  // create a new position
  await provider.send(tx)
}

main().then(
  () => process.exit(),
  (err) => {
    console.error(err);
    process.exit(-1);
  }
);