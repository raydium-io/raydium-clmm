import * as anchor from '@project-serum/anchor'
import { Program, web3 } from '@project-serum/anchor'
import * as chai from 'chai'
import chaiAsPromised from 'chai-as-promised'
chai.use(chaiAsPromised)
import { CyclosCore } from '../target/types/cyclos_core'

console.log('start');
(async () => {
  console.log('start')
  const keypair = new web3.Keypair()
  const wallet = new anchor.Wallet(keypair)
  const owner = wallet.publicKey
  const connection = new web3.Connection('https://dawn-red-log.solana-mainnet.quiknode.pro/ff88020a7deb8e7d855ad7c5125f489ef1e9db71/')
  const provider = new anchor.Provider(connection, wallet, {})
  console.log('setting provider')
  anchor.setProvider(provider)
  console.log('provider set, constructing program')

  try {
    const coreProgram = anchor.workspace.CyclosCore as Program<CyclosCore>

    console.log('fetching')
    const tickData = await coreProgram.account.tickState.fetch(new web3.PublicKey('AYJVr1hRGsTM1toYBcSP1dSxA7kSsMMEsRrwzFCMR467'))
    console.log('tick data', tickData)
  } catch(error) {
      console.log(error)
  }

})()