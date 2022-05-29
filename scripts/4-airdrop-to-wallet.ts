// Airdrop SOL and tokens to any wallet using
// `ts-node 4-airdrop-to-wallet 4c3G7V1DNx8LHQ5bwM7okugdxB1Q1caXmtbfDCZfXBLH`

import * as anchor from '@project-serum/anchor'
import { Keypair, Transaction, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import { web3 } from '@project-serum/anchor'
import keypairFile from './keypair.json'
import * as SPLToken from "@solana/spl-token";

export const FAUCET_AUTHORITY = Keypair.fromSecretKey(
  Uint8Array.from([166, 35, 198, 106, 198, 244, 143, 224, 64, 125, 232, 144, 28, 45, 178, 146, 56, 92, 99, 244, 25, 75, 104, 247, 215, 33, 62, 30, 186, 249, 163, 48, 185, 210, 115, 123, 192, 235, 130, 28, 35, 27, 9, 65, 38, 210, 100, 190, 62, 225, 55, 90, 209, 0, 227, 160, 141, 54, 132, 242, 98, 240, 212, 95])
);
const usdcMint = new web3.PublicKey('GyH7fsFCvD1Wt8DbUGEk6Hzt68SVqwRKDHSvyBS16ZHm')
const usdtMint = new web3.PublicKey('7HvgZSj1VqsGADkpb8jLXCVqyzniDHP5HzQCymHnrn1t')
const wsolMint = new web3.PublicKey('EC1x3JZ1PBW4MqH711rqfERaign6cxLTBNb3mi5LK9vP')
const cysMint = new web3.PublicKey('cxWg5RTK5AiSbBZh7NRg5btsbSrc8ETLXGf7tk3MUez')

async function main() {

  const keypair = web3.Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)))
  console.log('pubkey', keypair.publicKey.toString())
  const wallet = new anchor.Wallet(keypair)
  const owner = wallet.publicKey
  const connection = new web3.Connection('http://127.0.0.1:8899')
  const provider = new anchor.Provider(connection, wallet, {})
  anchor.setProvider(provider)

  console.log('entered pubkey', process.argv[2])
  const dest = new web3.PublicKey(process.argv[2])

  await connection.requestAirdrop(dest, 10 * LAMPORTS_PER_SOL)

  const tx = new web3.Transaction()
  tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash

  const usdcAccount = await SPLToken.Token.getAssociatedTokenAddress(
    SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
    SPLToken.TOKEN_PROGRAM_ID,
    usdcMint,
    dest
  )
  const usdtAccount = await SPLToken.Token.getAssociatedTokenAddress(
    SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
    SPLToken.TOKEN_PROGRAM_ID,
    usdtMint,
    dest
  )
  const wsolAccount = await SPLToken.Token.getAssociatedTokenAddress(
    SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
    SPLToken.TOKEN_PROGRAM_ID,
    wsolMint,
    dest
  )
  const cysAccount = await SPLToken.Token.getAssociatedTokenAddress(
    SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
    SPLToken.TOKEN_PROGRAM_ID,
    cysMint,
    dest
  )
  tx.instructions = [
    SPLToken.Token.createAssociatedTokenAccountInstruction(
      SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
      SPLToken.TOKEN_PROGRAM_ID,
      usdcMint,
      usdcAccount,
      dest,
      wallet.publicKey
    ),
    SPLToken.Token.createMintToInstruction(
      SPLToken.TOKEN_PROGRAM_ID,
      usdcMint,
      usdcAccount,
      FAUCET_AUTHORITY.publicKey,
      [],
      1_000_000_000_000
    ),
    SPLToken.Token.createAssociatedTokenAccountInstruction(
      SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
      SPLToken.TOKEN_PROGRAM_ID,
      usdtMint,
      usdtAccount,
      dest,
      wallet.publicKey
    ),
    SPLToken.Token.createMintToInstruction(
      SPLToken.TOKEN_PROGRAM_ID,
      usdtMint,
      usdtAccount,
      FAUCET_AUTHORITY.publicKey,
      [],
      1_000_000_000_000
    ),
    // SPLToken.Token.createAssociatedTokenAccountInstruction(
    //   SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
    //   SPLToken.TOKEN_PROGRAM_ID,
    //   wsolMint,
    //   wsolAccount,
    //   dest,
    //   wallet.publicKey
    // ),
    // SPLToken.Token.createMintToInstruction(
    //   SPLToken.TOKEN_PROGRAM_ID,
    //   wsolMint,
    //   wsolAccount,
    //   FAUCET_AUTHORITY.publicKey,
    //   [],
    //   100_000_000_000
    // ),
    SPLToken.Token.createAssociatedTokenAccountInstruction(
      SPLToken.ASSOCIATED_TOKEN_PROGRAM_ID,
      SPLToken.TOKEN_PROGRAM_ID,
      cysMint,
      cysAccount,
      dest,
      wallet.publicKey
    ),
    SPLToken.Token.createMintToInstruction(
      SPLToken.TOKEN_PROGRAM_ID,
      cysMint,
      cysAccount,
      FAUCET_AUTHORITY.publicKey,
      [],
      1_000_000_000_000
    )
  ]
  await provider.send(tx, [FAUCET_AUTHORITY])
}

main().then(
  () => process.exit(),
  (err) => {
    console.error(err);
    process.exit(-1);
  }
);