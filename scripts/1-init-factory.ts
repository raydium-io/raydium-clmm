import * as anchor from '@project-serum/anchor'
import { Program, web3, BN, ProgramError } from '@project-serum/anchor'
import { assert, expect } from 'chai'
import * as chai from 'chai'
import chaiAsPromised from 'chai-as-promised'
chai.use(chaiAsPromised)
import { CyclosCore } from '../target/types/cyclos_core'
import keypairFile from './keypair.json';

(async () => {
  const keypair = web3.Keypair.fromSeed(Uint8Array.from(keypairFile.slice(0, 32)))
  console.log('pubkey', keypair.publicKey.toString())
  const wallet = new anchor.Wallet(keypair)
  const owner = wallet.publicKey
  const connection = new web3.Connection('http://127.0.0.1:8899')
  const provider = new anchor.Provider(connection, wallet, {})
  anchor.setProvider(provider)

  const coreProgram = anchor.workspace.CyclosCore as Program<CyclosCore>

  const [factoryState, factoryStateBump] = await web3.PublicKey.findProgramAddress([], coreProgram.programId)
  const tx = coreProgram.transaction.initFactory({
    accounts: {
      owner,
      factoryState,
      systemProgram: web3.SystemProgram.programId,
    }
  })
  await provider.send(tx)

  // verify
  const factoryStateData = await coreProgram.account.factoryState.fetch(factoryState)
  assert.equal(factoryStateData.bump, factoryStateBump)
  assert(factoryStateData.owner.equals(owner))
})()