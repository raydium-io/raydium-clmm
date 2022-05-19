import * as anchor from '@project-serum/anchor'
import { Program, web3, BN, ProgramError } from '@project-serum/anchor'
import { assert, expect } from 'chai'
import * as chai from 'chai'
import chaiAsPromised from 'chai-as-promised'
import { FEE_SEED, u32ToSeed } from '@cykura/sdk'
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

    const feeTiers = [{
        // super stable
        fee: 20,
        tickSpacing: 1
    },{
        // turbo spl
        fee: 80,
        tickSpacing: 60
    },
    {
        fee: 500,
        tickSpacing: 10
    },
    {
        fee: 3000,
        tickSpacing: 60
    },
    {
        fee: 10_000,
        tickSpacing: 200
    }
    ]
    for (let { fee, tickSpacing } of feeTiers) {
        const [feeState, feeStateBump] = await web3.PublicKey.findProgramAddress(
            [FEE_SEED, u32ToSeed(fee)],
            coreProgram.programId
        );
        const tx = coreProgram.transaction.enableFeeAmount(fee, tickSpacing, {
            accounts: {
                owner,
                factoryState,
                feeState,
                systemProgram: web3.SystemProgram.programId,
            }
        })
        await provider.send(tx)

        // verify
        const feeStateData = await coreProgram.account.feeState.fetch(feeState)
        assert.equal(feeStateData.bump, feeStateBump)
        assert.equal(feeStateData.fee, fee)
        assert.equal(feeStateData.tickSpacing, tickSpacing)
    }
})()