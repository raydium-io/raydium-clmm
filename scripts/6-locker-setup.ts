import { GokiSDK } from '@gokiprotocol/client'
import * as anchor from '@project-serum/anchor'
import { web3 } from '@project-serum/anchor'
import { PublicKey, SolanaProvider } from '@saberhq/solana-contrib'
import { ASSOCIATED_TOKEN_PROGRAM_ID, createMintInstructions, getATAAddress, TOKEN_PROGRAM_ID } from '@saberhq/token-utils'
import { Token } from '@solana/spl-token'
import { Keypair, SystemProgram, Transaction } from "@solana/web3.js"
import { findEscrowAddress, findGovernorAddress, findLockerAddress, LockerWrapper, TribecaSDK } from '@tribecahq/tribeca-sdk'
import keypairFile from './keypair.json'
import type { Provider } from "@saberhq/solana-contrib";
import * as SPLToken from "@solana/spl-token";

export async function createMint(
  provider: Provider,
  connection: web3.Connection,
  authority?: PublicKey,
  decimals?: number
): Promise<PublicKey> {
  if (authority === undefined) {
    authority = provider.wallet.publicKey;
  }
  const mint = Keypair.fromSecretKey(
    Uint8Array.from([170, 204, 133, 206, 215, 135, 147, 69, 202, 136, 132, 212, 28, 149, 110, 252, 100, 236, 7, 172, 87, 170, 80, 207, 122, 181, 91, 120, 31, 198, 72, 62, 9, 54, 24, 114, 208, 200, 16, 126, 237, 6, 101, 43, 79, 108, 255, 88, 254, 188, 218, 124, 116, 214, 182, 25, 219, 28, 183, 227, 101, 197, 44, 71])
  ); // cxWg5RTK5AiSbBZh7NRg5btsbSrc8ETLXGf7tk3MUez

  const tx = new Transaction();
  tx.add(
    // create account
    SystemProgram.createAccount({
      fromPubkey: provider.wallet.publicKey,
      newAccountPubkey: mint.publicKey,
      space: SPLToken.MintLayout.span,
      lamports: await provider.connection.getMinimumBalanceForRentExemption(
        SPLToken.MintLayout.span
      ),
      programId: TOKEN_PROGRAM_ID,
    }),
    // init mint
    SPLToken.Token.createInitMintInstruction(
      TOKEN_PROGRAM_ID,
      mint.publicKey,
      decimals ?? 6,
      authority,
      null// freeze authority (we use null first, the auth can let you freeze user's token account)
    )
  );
  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;

  await provider.send(tx, [mint]);

  return mint.publicKey;
}
export const MINT_AUTHORITY = Keypair.fromSecretKey(
  Uint8Array.from([166, 35, 198, 106, 198, 244, 143, 224, 64, 125, 232, 144, 28, 45, 178, 146, 56, 92, 99, 244, 25, 75, 104, 247, 215, 33, 62, 30, 186, 249, 163, 48, 185, 210, 115, 123, 192, 235, 130, 28, 35, 27, 9, 65, 38, 210, 100, 190, 62, 225, 55, 90, 209, 0, 227, 160, 141, 54, 132, 242, 98, 240, 212, 95])
);


async function main() {

  const keypair = web3.Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)))
  console.log('6-pubkey', keypair.publicKey.toString())
  const wallet = new anchor.Wallet(keypair)
  const connection = new web3.Connection('http://127.0.0.1:8899')
  const anchorProvider = new anchor.Provider(connection, wallet, {})
  anchor.setProvider(anchorProvider)

  const solanaProvider = SolanaProvider.init({
    connection,
    wallet,
    opts: {},
  })

  // base address to derive smart wallet and governor addresses
  const base = web3.Keypair.generate()

  const numOwners = 10
  const ownerA = web3.Keypair.generate()
  // const owners = [ownerA.publicKey];
  const threshold = new anchor.BN(1)

  const lockedAmt = new anchor.BN(1e6)

  // derive addresses -----------------

  const [governorKey] = await findGovernorAddress(base.publicKey)
  const [lockerKey] = await findLockerAddress(base.publicKey)
  const [escrowKey] = await findEscrowAddress(lockerKey, anchorProvider.wallet.publicKey)

  const owners = [governorKey]

  // setup goki multisig smart wallet -----------------

  const gokiSdk = GokiSDK.load({ provider: solanaProvider })

  const { smartWalletWrapper, tx: createSmartWalletTx } = await gokiSdk.newSmartWallet(
    {
      numOwners,
      owners,
      threshold,
      base,
    }
  )
  // createSmartWalletTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
  // let txSig = (await createSmartWalletTx.confirm()).signature;
  let txBuild = createSmartWalletTx.build();
  txBuild.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  let txSig = await anchorProvider.send(txBuild, createSmartWalletTx.signers)
  console.log(`create new smartWallet: ${txSig}`);

  // governance token setup -------------------

  // const govTokenMint = await createMint(gokiSdk.provider, connection)
  let cysMint = Keypair.fromSecretKey(
    Uint8Array.from([170, 204, 133, 206, 215, 135, 147, 69, 202, 136, 132, 212, 28, 149, 110, 252, 100, 236, 7, 172, 87, 170, 80, 207, 122, 181, 91, 120, 31, 198, 72, 62, 9, 54, 24, 114, 208, 200, 16, 126, 237, 6, 101, 43, 79, 108, 255, 88, 254, 188, 218, 124, 116, 214, 182, 25, 219, 28, 183, 227, 101, 197, 44, 71])
  ); // cxWg5RTK5AiSbBZh7NRg5btsbSrc8ETLXGf7tk3MUez
  const govTokenMint = cysMint.publicKey;
  const cysTx = new Transaction();  
  cysTx.add(
    // create account
    SystemProgram.createAccount({
      fromPubkey: wallet.publicKey,
      newAccountPubkey: cysMint.publicKey,
      space: SPLToken.MintLayout.span,
      lamports: await SPLToken.Token.getMinBalanceRentForExemptMint(connection),
      programId: SPLToken.TOKEN_PROGRAM_ID,
    }),
    // init mint
    SPLToken.Token.createInitMintInstruction(
      SPLToken.TOKEN_PROGRAM_ID, // program id, always token program id
      cysMint.publicKey, // mint account public key
      6, // decimals
      MINT_AUTHORITY.publicKey, // mint authority (an auth to mint token)
      null // freeze authority (we use null first, the auth can let you freeze user's token account)
    )
  );

  const txhash = await anchorProvider.send(cysTx, [cysMint])
  console.log(`txhash: ${txhash}`);
  

  // Governor and locker setup ---------------------

  const tribecaSdk = TribecaSDK.load({ provider: solanaProvider })

  // create governor
  const { wrapper: governorWrapper , tx: createGovernorTx } = await tribecaSdk.govern.createGovernor({
    electorate: lockerKey,
    smartWallet: smartWalletWrapper.key,
    baseKP: base,
  })
  txBuild = createGovernorTx.build();
  txBuild.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  txSig = await anchorProvider.send(txBuild, createGovernorTx.signers)
  console.log(`create new governor: ${txSig}`);

  // create locker
  const { locker, tx: createLockerTx } = await tribecaSdk.createLocker({
    baseKP: base,
    governor: governorKey,
    govTokenMint,
    minStakeDuration: new anchor.BN(1),
  })
  txBuild = createLockerTx.build();
  txBuild.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  txSig = await anchorProvider.send(txBuild, createLockerTx.signers)
  console.log(`create new locker: ${txSig}`);

  const lockerWrapper = await LockerWrapper.load(
    tribecaSdk,
    lockerKey,
    governorKey
  )

  const eleData = await lockerWrapper.data()

  console.log(
    eleData.base.toString(),
    "\ntokenMint: ", eleData.tokenMint.toString(),
    eleData.governor.toString(),
    eleData.lockedSupply.toString()
  )

}

main().then(
  () => process.exit(),
  (err) => {
    console.error(err)
    process.exit(-1)
  }
)