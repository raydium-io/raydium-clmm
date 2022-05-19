import * as anchor from '@project-serum/anchor'
import { Program, web3, BN, ProgramError } from '@project-serum/anchor'
import * as metaplex from '@metaplex/js'
import { Token, TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID } from '@solana/spl-token'
import {
  Pool,
  BITMAP_SEED,
  FEE_SEED,
  OBSERVATION_SEED,
  POOL_SEED,
  POSITION_SEED,
  TICK_SEED,
  u16ToSeed,
  u32ToSeed
} from '@cykura/sdk'
import { CurrencyAmount, Token as UniToken } from '@cykura/sdk-core'
import { assert, expect } from 'chai'
import * as chai from 'chai'
import chaiAsPromised from 'chai-as-promised'
chai.use(chaiAsPromised)

import { CyclosCore } from '../target/types/cyclos_core'
import {
  MaxU64,
  MAX_SQRT_RATIO,
  MAX_TICK,
  MIN_SQRT_RATIO,
  MIN_TICK,
} from './utils'
import SolanaTickDataProvider from './SolanaTickDataProvider'
import { Transaction } from '@solana/web3.js'
import JSBI from 'jsbi'

console.log('starting test')
const { metadata: { Metadata } } = metaplex.programs

const { PublicKey, Keypair, SystemProgram } = anchor.web3

describe('cyclos-core', async () => {
  console.log('in describe')

  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());
  console.log('provider set')

  const coreProgram = anchor.workspace.CyclosCore as Program<CyclosCore>
  console.log('program created')
  const { connection, wallet } = anchor.getProvider()
  const owner = anchor.getProvider().wallet.publicKey
  console.log('owner key', owner.toString())
  const notOwner = new Keypair()

  const fee = 500;
  const tickSpacing = 10;

  const [factoryState, factoryStateBump] = await PublicKey.findProgramAddress([], coreProgram.programId)

  const [feeState, feeStateBump] = await PublicKey.findProgramAddress(
    [FEE_SEED, u32ToSeed(fee)],
    coreProgram.programId
  );
  console.log("Fee", feeState.toString(), feeStateBump)

  const mintAuthority = new Keypair()

  // Tokens constituting the pool
  let token0: Token
  let token1: Token
  let token2: Token

  let uniToken0: UniToken
  let uniToken1: UniToken
  let uniToken2: UniToken

  let uniPoolA: Pool

  // ATAs to hold pool tokens
  let vaultA0: web3.PublicKey
  let vaultA1: web3.PublicKey
  let vaultB1: web3.PublicKey
  let vaultB2: web3.PublicKey

  let poolAState: web3.PublicKey
  let poolAStateBump: number
  let poolBState: web3.PublicKey
  let poolBStateBump: number

  let initialObservationStateA: web3.PublicKey
  let initialObservationBumpA: number
  let initialObservationStateB: web3.PublicKey
  let initialObservationBumpB: number

  // These accounts will spend tokens to mint the position
  let minterWallet0: web3.PublicKey
  let minterWallet1: web3.PublicKey
  let minterWallet2: web3.PublicKey

  let temporaryNftHolder: web3.PublicKey

  const tickLower = 0
  const tickUpper = 10
  const wordPosLower = (tickLower / tickSpacing) >> 8
  const wordPosUpper = (tickUpper / tickSpacing) >> 8

  const amount0Desired = new BN(1_000_000)
  const amount1Desired = new BN(1_000_000)
  const amount0Minimum = new BN(0)
  const amount1Minimum = new BN(0)

  const nftMintAKeypair = new Keypair()
  const nftMintBKeypair = new web3.Keypair()

  let tickLowerAState: web3.PublicKey
  let tickLowerAStateBump: number
  let tickLowerBState: web3.PublicKey
  let tickLowerBStateBump: number
  let tickUpperAState: web3.PublicKey
  let tickUpperAStateBump: number
  let tickUpperBState: web3.PublicKey
  let tickUpperBStateBump: number
  let corePositionAState: web3.PublicKey
  let corePositionABump: number
  let corePositionBState: web3.PublicKey
  let corePositionBBump: number
  let bitmapLowerAState: web3.PublicKey
  let bitmapLowerABump: number
  let bitmapLowerBState: web3.PublicKey
  let bitmapLowerBBump: number
  let bitmapUpperAState: web3.PublicKey
  let bitmapUpperABump: number
  let bitmapUpperBState: web3.PublicKey
  let bitmapUpperBBump: number
  let tokenizedPositionAState: web3.PublicKey
  let tokenizedPositionABump: number
  let tokenizedPositionBState: web3.PublicKey
  let tokenizedPositionBBump: number
  let positionANftAccount: web3.PublicKey
  let positionBNftAccount: web3.PublicKey
  let metadataAccount: web3.PublicKey
  let lastObservationAState: web3.PublicKey
  let nextObservationAState: web3.PublicKey
  let latestObservationBState: web3.PublicKey
  let nextObservationBState: web3.PublicKey

  const protocolFeeRecipient = new Keypair()
  let feeRecipientWallet0: web3.PublicKey
  let feeRecipientWallet1: web3.PublicKey

  const initialPriceX32 = new BN(4297115210)
  const initialTick = 10

  console.log('before token test')
  it('Create token mints', async () => {
    console.log('creating tokens')
    const transferSolTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: mintAuthority.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    )
    transferSolTx.add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: notOwner.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    )
    await anchor.getProvider().send(transferSolTx)

    token0 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    )
    token1 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    )
    token2 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    )

    if (token0.publicKey.toString() > token1.publicKey.toString()) { // swap token mints
      console.log('Swap tokens for A')
      const temp = token0
      token0 = token1
      token1 = temp
    }

    uniToken0 = new UniToken(0, token0.publicKey, 8)
    uniToken1 = new UniToken(0, token1.publicKey, 8)
    uniToken2 = new UniToken(0, token2.publicKey, 8)
    console.log('Token 0', token0.publicKey.toString())
    console.log('Token 1', token1.publicKey.toString())
    console.log('Token 2', token2.publicKey.toString())

    while (token1.publicKey.toString() > token2.publicKey.toString()) {
      token2 = await Token.createMint(
        connection,
        mintAuthority,
        mintAuthority.publicKey,
        null,
        8,
        TOKEN_PROGRAM_ID
      )
    }
  })

  it('creates token accounts for position minter and airdrops to them', async () => {
    minterWallet0 = await token0.createAssociatedTokenAccount(owner)
    minterWallet1 = await token1.createAssociatedTokenAccount(owner)
    minterWallet2 = await token2.createAssociatedTokenAccount(owner)
    await token0.mintTo(minterWallet0, mintAuthority, [], 100_000_000)
    await token1.mintTo(minterWallet1, mintAuthority, [], 100_000_000)
    await token2.mintTo(minterWallet2, mintAuthority, [], 100_000_000)
  })

  it('derive pool address', async () => {
    [poolAState, poolAStateBump] = await PublicKey.findProgramAddress(
      [
        POOL_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee)
      ],
      coreProgram.programId
    )
    console.log('got pool address', poolAState);

    [poolBState, poolBStateBump] = await PublicKey.findProgramAddress(
      [
        POOL_SEED,
        token1.publicKey.toBuffer(),
        token2.publicKey.toBuffer(),
        u32ToSeed(fee)
      ],
      coreProgram.programId
    )
    console.log('got pool address', poolBState)
  })

  it('derive vault addresses', async () => {
    vaultA0 = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token0.publicKey,
      poolAState,
      true
    )
    vaultA1 = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token1.publicKey,
      poolAState,
      true
    )
    vaultB1 = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token1.publicKey,
      poolBState,
      true
    )
    vaultB2 = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token2.publicKey,
      poolBState,
      true
    )

    const createAtaTx = new Transaction()
    createAtaTx.instructions = [
      Token.createAssociatedTokenAccountInstruction(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        token0.publicKey,
        vaultA0,
        poolAState,
        owner
      ),
      Token.createAssociatedTokenAccountInstruction(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        token1.publicKey,
        vaultA1,
        poolAState,
        owner
      ),
      Token.createAssociatedTokenAccountInstruction(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        token1.publicKey,
        vaultB1,
        poolBState,
        owner
      ),
      Token.createAssociatedTokenAccountInstruction(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        token2.publicKey,
        vaultB2,
        poolBState,
        owner
      ),
    ]
    createAtaTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
    await anchor.getProvider().send(createAtaTx)
  })

  describe('#init_factory', () => {

    // Test for event and owner value
    it('initializes factory and emits an event', async () => {
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("OwnerChanged", (event, slot) => {
          assert((event.oldOwner as web3.PublicKey).equals(new PublicKey(0)))
          assert((event.newOwner as web3.PublicKey).equals(owner))

          resolve([event, slot]);
        });

        coreProgram.rpc.initFactory({
          accounts: {
            owner,
            factoryState,
            systemProgram: SystemProgram.programId,
          }
        });
      });
      await coreProgram.removeEventListener(listener);

      const factoryStateData = await coreProgram.account.factoryState.fetch(factoryState)
      assert.equal(factoryStateData.bump, factoryStateBump)
      assert(factoryStateData.owner.equals(owner))
      assert.equal(factoryStateData.feeProtocol, 3)
    });

    it('Trying to re-initialize factory fails', async () => {
      await expect(coreProgram.rpc.initFactory({
        accounts: {
          owner,
          factoryState,
          systemProgram: anchor.web3.SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    });
  })

  describe('#set_owner', () => {
    const newOwner = new Keypair()

    it('fails if owner does not sign', async () => {
      const tx = coreProgram.transaction.setOwner({
        accounts: {
          owner,
          newOwner: newOwner.publicKey,
          factoryState,
        }
      });
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash

      await expect(connection.sendTransaction(tx, [])).to.be.rejectedWith(Error)
    })

    it('fails if caller is not owner', async () => {
      const tx = coreProgram.transaction.setOwner({
        accounts: {
          owner,
          newOwner: newOwner.publicKey,
          factoryState,
        }
      });
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash

      await expect(connection.sendTransaction(tx, [notOwner])).to.be.rejectedWith(Error)
    })

    it('fails if correct signer but incorrect owner field', async () => {
      await expect(coreProgram.rpc.setOwner({
        accounts: {
          owner: notOwner.publicKey,
          newOwner: newOwner.publicKey,
          factoryState,
        }
      })).to.be.rejectedWith(Error)
    })

    // Test for event and updated owner value
    it('updates owner and emits an event', async function () {
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("OwnerChanged", (event, slot) => {
          assert((event.oldOwner as web3.PublicKey).equals(owner))
          assert((event.newOwner as web3.PublicKey).equals(newOwner.publicKey))

          resolve([event, slot]);
        });

        coreProgram.rpc.setOwner({
          accounts: {
            owner,
            newOwner: newOwner.publicKey,
            factoryState,
          }
        });
      });
      await coreProgram.removeEventListener(listener);

      const factoryStateData = await coreProgram.account.factoryState.fetch(factoryState)
      assert(factoryStateData.owner.equals(newOwner.publicKey))
    })

    it('reverts to original owner when signed by the new owner', async () => {
      await coreProgram.rpc.setOwner({
        accounts: {
          owner: newOwner.publicKey,
          newOwner: owner,
          factoryState,
        }, signers: [newOwner]
      });
      const factoryStateData = await coreProgram.account.factoryState.fetch(factoryState)
      assert(factoryStateData.owner.equals(owner))
    })
  })

  describe('#enable_fee_amount', () => {
    it('fails if PDA seeds do not match', async () => {
      await expect(coreProgram.rpc.enableFeeAmount(fee + 1, tickSpacing, {
        accounts: {
          owner,
          factoryState,
          feeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if caller is not owner', async () => {
      const tx = coreProgram.transaction.enableFeeAmount(fee, tickSpacing, {
        accounts: {
          owner: notOwner.publicKey,
          factoryState,
          feeState,
          systemProgram: SystemProgram.programId,
        }, signers: [notOwner]
      })
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash

      await expect(connection.sendTransaction(tx, [notOwner])).to.be.rejectedWith(Error)
    })

    it('fails if fee is too great', async () => {
      const highFee = 1_000_000
      const [highFeeState, highFeeStateBump] = await PublicKey.findProgramAddress(
        [FEE_SEED, u32ToSeed(highFee)],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.enableFeeAmount(highFee, tickSpacing, {
        accounts: {
          owner,
          factoryState,
          feeState: highFeeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if tick spacing is too small', async () => {
      await expect(coreProgram.rpc.enableFeeAmount(fee, 0, {
        accounts: {
          owner,
          factoryState,
          feeState: feeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if tick spacing is too large', async () => {
      await expect(coreProgram.rpc.enableFeeAmount(fee, 16384, {
        accounts: {
          owner,
          factoryState,
          feeState: feeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('sets the fee amount and emits an event', async () => {
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("FeeAmountEnabled", (event, slot) => {
          assert.equal(event.fee, fee)
          assert.equal(event.tickSpacing, tickSpacing)

          resolve([event, slot]);
        });

        coreProgram.rpc.enableFeeAmount(fee, tickSpacing, {
          accounts: {
            owner,
            factoryState,
            feeState,
            systemProgram: SystemProgram.programId,
          }
        })
      });
      await coreProgram.removeEventListener(listener);

      const feeStateData = await coreProgram.account.feeState.fetch(feeState)
      console.log('fee state', feeStateData)
      assert.equal(feeStateData.bump, feeStateBump)
      assert.equal(feeStateData.fee, fee)
      assert.equal(feeStateData.tickSpacing, tickSpacing)
    })

    it('fails if already initialized', async () => {
      await expect(coreProgram.rpc.enableFeeAmount(feeStateBump, fee, tickSpacing, {
        accounts: {
          owner,
          factoryState,
          feeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('cannot change spacing of a fee tier', async () => {
      await expect(coreProgram.rpc.enableFeeAmount(feeStateBump, fee, tickSpacing + 1, {
        accounts: {
          owner,
          factoryState,
          feeState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })
  })

  describe('#create_and_init_pool', () => {
    it('derive first observation slot address', async () => {
      [initialObservationStateA, initialObservationBumpA] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(0)
        ],
        coreProgram.programId
      );
      [initialObservationStateB, initialObservationBumpB] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token1.publicKey.toBuffer(),
          token2.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(0)
        ],
        coreProgram.programId
      )
    })

    it('fails if tokens are passed in reverse', async () => {
      // Unlike Uniswap, we must pass the tokens by address sort order
      await expect(coreProgram.rpc.createAndInitPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          token0: token1.publicKey,
          token1: token0.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if token0 == token1', async () => {
      // Unlike Uniswap, we must pass the tokens by address sort order
      await expect(coreProgram.rpc.createAndInitPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token0.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if fee amount is not enabled', async () => {
      const [uninitializedFeeState, _] = await PublicKey.findProgramAddress(
        [FEE_SEED, u32ToSeed(fee + 1)],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.createAndInitPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token0.publicKey,
          feeState: uninitializedFeeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if starting price is too low', async () => {
      await expect(coreProgram.rpc.createAndInitPool(new BN(1), {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)

      await expect(coreProgram.rpc.createAndInitPool(
        MIN_SQRT_RATIO.subn(1), {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)

    })

    it('fails if starting price is too high', async () => {
      await expect(coreProgram.rpc.createAndInitPool(MAX_SQRT_RATIO, {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)

      await expect(coreProgram.rpc.createAndInitPool(
        new BN(2).pow(new BN(64)).subn(1), { // u64::MAX
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })

    it('creates a new pool and initializes it with a starting price', async () => {
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("PoolCreatedAndInitialized", (event, slot) => {
          assert((event.token0 as web3.PublicKey).equals(token0.publicKey))
          assert((event.token1 as web3.PublicKey).equals(token1.publicKey))
          assert.equal(event.fee, fee)
          assert.equal(event.tickSpacing, tickSpacing)
          assert((event.poolState as web3.PublicKey).equals(poolAState))
          assert((event.sqrtPriceX32 as BN).eq(initialPriceX32))
          assert.equal(event.tick, initialTick)

          resolve([event, slot]);
        });

        coreProgram.rpc.createAndInitPool(initialPriceX32, {
          accounts: {
            poolCreator: owner,
            token0: token0.publicKey,
            token1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          }
        })
      })
      await coreProgram.removeEventListener(listener)

      // pool state variables
      const poolStateData = await coreProgram.account.poolState.fetch(poolAState)
      assert.equal(poolStateData.bump, poolAStateBump)
      assert((poolStateData.token0).equals(token0.publicKey))
      assert((poolStateData.token1).equals(token1.publicKey))
      assert.equal(poolStateData.fee, fee)
      assert.equal(poolStateData.tickSpacing, tickSpacing)
      assert(poolStateData.liquidity.eqn(0))
      assert((poolStateData.sqrtPriceX32).eq(initialPriceX32))
      assert.equal(poolStateData.tick, initialTick)
      assert.equal(poolStateData.observationIndex, 0)
      assert.equal(poolStateData.observationCardinality, 1)
      assert.equal(poolStateData.observationCardinalityNext, 1)
      assert(poolStateData.feeGrowthGlobal0X32.eq(new BN(0)))
      assert(poolStateData.feeGrowthGlobal1X32.eq(new BN(0)))
      assert(poolStateData.protocolFeesToken0.eq(new BN(0)))
      assert(poolStateData.protocolFeesToken1.eq(new BN(0)))
      assert(poolStateData.unlocked)

      // first observations slot
      const observationStateData = await coreProgram.account.observationState.fetch(initialObservationStateA)
      assert.equal(observationStateData.bump, initialObservationBumpA)
      assert.equal(observationStateData.index, 0)
      assert(observationStateData.tickCumulative.eqn(0))
      assert(observationStateData.secondsPerLiquidityCumulativeX32.eqn(0))
      assert(observationStateData.initialized)
      assert.approximately(observationStateData.blockTimestamp, Math.floor(Date.now() / 1000), 60)

      console.log('got pool address', poolAState.toString())
    })

    it('fails if already initialized', async () => {
      await expect(coreProgram.rpc.createAndInitPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          token0: token0.publicKey,
          token1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })
  })

  describe('#increase_observation_cardinality_next', () => {
    it('fails if bump does not produce a PDA with observation state seeds', async () => {
      const [observationState, _] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(1)
        ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([0]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: observationState,
          isSigner: true,
          isWritable: true
        }]
      })).to.be.rejectedWith(Error)

    })

    it('fails if bump is valid but account does not match expected address for current cardinality_next', async () => {
      const [_, observationStateBump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(1)
        ],
        coreProgram.programId
      )
      const fakeAccount = new Keypair()

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationStateBump]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: fakeAccount.publicKey,
          isSigner: true,
          isWritable: true
        }], signers: [fakeAccount]
      })).to.be.rejectedWith(Error)
    })

    it('fails if a single address is passed with index greater than cardinality_next', async () => {
      const [observationState2, observationState2Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(2)
        ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationState2Bump]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: observationState2,
          isSigner: false,
          isWritable: true
        }]
      })).to.be.rejectedWith(Error)
    })

    it('increase cardinality by one', async () => {
      const [observationState0, observationState0Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(0)
        ],
        coreProgram.programId
      )
      const firstObservtionBefore = await coreProgram.account.observationState.fetch(observationState0)

      const [observationState1, observationState1Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(1)
        ],
        coreProgram.programId
      )

      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("IncreaseObservationCardinalityNext", (event, slot) => {
          assert.equal(event.observationCardinalityNextOld, 1)
          assert.equal(event.observationCardinalityNextNew, 2)
          resolve([event, slot]);
        });

        coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationState1Bump]), {
          accounts: {
            payer: owner,
            poolState: poolAState,
            systemProgram: SystemProgram.programId,
          }, remainingAccounts: [{
            pubkey: observationState1,
            isSigner: false,
            isWritable: true
          }]
        })
      })
      await coreProgram.removeEventListener(listener)

      const observationState1Data = await coreProgram.account.observationState.fetch(observationState1)
      console.log('Observation state 1 data', observationState1Data)
      assert.equal(observationState1Data.bump, observationState1Bump)
      assert.equal(observationState1Data.index, 1)
      assert.equal(observationState1Data.blockTimestamp, 1)
      assert(observationState1Data.tickCumulative.eqn(0))
      assert(observationState1Data.secondsPerLiquidityCumulativeX32.eqn(0))
      assert.isFalse(observationState1Data.initialized)

      const poolStateData = await coreProgram.account.poolState.fetch(poolAState)
      assert.equal(poolStateData.observationIndex, 0)
      assert.equal(poolStateData.observationCardinality, 1)
      assert.equal(poolStateData.observationCardinalityNext, 2)

      // does not touch the first observation
      const firstObservtionAfter = await coreProgram.account.observationState.fetch(observationState0)
      assert.deepEqual(firstObservtionAfter, firstObservtionBefore)
    })

    it('fails if accounts are not in ascending order of index', async () => {
      const [observationState2, observationState2Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(2)
        ],
        coreProgram.programId
      )
      const [observationState3, observationState3Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(3)
        ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationState3Bump, observationState2Bump]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: observationState3,
          isSigner: false,
          isWritable: true
        },
        {
          pubkey: observationState2,
          isSigner: false,
          isWritable: true
        }]
      })).to.be.rejectedWith(Error)
    })

    it('fails if a stray account is present between the array of observation accounts', async () => {
      const [observationState2, observationState2Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(2)
        ],
        coreProgram.programId
      )
      const [observationState3, observationState3Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(3)
        ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationState2Bump, observationState3Bump]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: observationState2,
          isSigner: false,
          isWritable: true
        },
        {
          pubkey: new Keypair().publicKey,
          isSigner: false,
          isWritable: true
        },
        {
          pubkey: observationState3,
          isSigner: false,
          isWritable: true
        }]
      })).to.be.rejectedWith(Error)
    })

    it('fails if less than current value of cardinality_next', async () => {
      const [observationState1, observationState1Bump] = await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(1)
        ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.increaseObservationCardinalityNext(Buffer.from([observationState1Bump]), {
        accounts: {
          payer: owner,
          poolState: poolAState,
          systemProgram: SystemProgram.programId,
        }, remainingAccounts: [{
          pubkey: observationState1,
          isSigner: false,
          isWritable: true
        }]
      })).to.be.rejectedWith(Error)
    })

  })

  describe('#set_fee_protocol', () => {
    it('cannot be changed by addresses that are not owner', async () => {
      await expect(coreProgram.rpc.setFeeProtocol(6, {
        accounts: {
          owner: notOwner.publicKey,
          factoryState,
        }, signers: [notOwner]
      })).to.be.rejectedWith(Error)
    })

    it('cannot be changed out of bounds', async () => {
      await expect(coreProgram.rpc.setFeeProtocol(1, {
        accounts: {
          owner,
          factoryState,
        }
      })).to.be.rejectedWith(Error)

      await expect(coreProgram.rpc.setFeeProtocol(11, {
        accounts: {
          owner,
          factoryState,
        }
      })).to.be.rejectedWith(Error)
    })

    it('can be changed by owner', async () => {
      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = coreProgram.addEventListener("SetFeeProtocolEvent", (event, slot) => {
      //     assert.equal(event.feeProtocolOld, 3)
      //     assert.equal(event.feeProtocol, 6)

      //     resolve([event, slot]);
      //   });

      //   coreProgram.rpc.setFeeProtocol(6, {
      //     accounts: {
      //       owner,
      //       factoryState,
      //     }
      //   })
      // })
      // await coreProgram.removeEventListener(listener)

      await coreProgram.rpc.setFeeProtocol(6, {
        accounts: {
          owner,
          factoryState,
        }
      })

      const factoryStateData = await coreProgram.account.factoryState.fetch(factoryState)
      assert.equal(factoryStateData.feeProtocol, 6)
    })
  })

  describe('#collect_protocol', () => {
    it('creates token accounts for recipient', async () => {
      feeRecipientWallet0 = await token0.createAssociatedTokenAccount(protocolFeeRecipient.publicKey)
      feeRecipientWallet1 = await token1.createAssociatedTokenAccount(protocolFeeRecipient.publicKey)
    })

    it('fails if caller is not owner', async () => {
      await expect(coreProgram.rpc.collectProtocol(MaxU64, MaxU64, {
        accounts: {
          owner: notOwner,
          factoryState,
          poolState: poolAState,
          vault0: vaultA0,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if vault 0 address is not valid', async () => {
      await expect(coreProgram.rpc.collectProtocol(MaxU64, MaxU64, {
        accounts: {
          owner: notOwner,
          factoryState,
          poolState: poolAState,
          vault0: new Keypair().publicKey,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if vault 1 address is not valid', async () => {
      await expect(coreProgram.rpc.collectProtocol(MaxU64, MaxU64, {
        accounts: {
          owner: notOwner,
          factoryState,
          poolState: poolAState,
          vault0: vaultA0,
          vault1: new Keypair().publicKey,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        }
      })).to.be.rejectedWith(Error)
    })

    it('no token transfers if no fees', async () => {
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("CollectProtocolEvent", (event, slot) => {
          assert((event.poolState as web3.PublicKey).equals(poolAState))
          assert((event.sender as web3.PublicKey).equals(owner))
          assert((event.amount0 as BN).eqn(0))
          assert((event.amount1 as BN).eqn(0))

          resolve([event, slot]);
        });

        coreProgram.rpc.collectProtocol(MaxU64, MaxU64, {
          accounts: {
            owner,
            factoryState,
            poolState: poolAState,
            vault0: vaultA0,
            vault1: vaultA1,
            recipientWallet0: feeRecipientWallet0,
            recipientWallet1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          }
        })
      })
      await coreProgram.removeEventListener(listener)

      const poolStateData = await coreProgram.account.poolState.fetch(poolAState)
      assert(poolStateData.protocolFeesToken0.eqn(0))
      assert(poolStateData.protocolFeesToken1.eqn(0))

      const recipientWallet0Info = await token0.getAccountInfo(feeRecipientWallet0)
      const recipientWallet1Info = await token1.getAccountInfo(feeRecipientWallet1)
      assert(recipientWallet0Info.amount.eqn(0))
      assert(recipientWallet1Info.amount.eqn(0))
    })

    // TODO remaining tests after swap component is ready

  })

  it('find program accounts addresses for position creation', async () => {
    [tickLowerAState, tickLowerAStateBump] = await PublicKey.findProgramAddress([
      TICK_SEED,
      token0.publicKey.toBuffer(),
      token1.publicKey.toBuffer(),
      u32ToSeed(fee),
      u32ToSeed(tickLower)
    ],
      coreProgram.programId
    );
    [tickLowerBState, tickLowerBStateBump] = await PublicKey.findProgramAddress([
      TICK_SEED,
      token1.publicKey.toBuffer(),
      token2.publicKey.toBuffer(),
      u32ToSeed(fee),
      u32ToSeed(tickLower)
    ],
      coreProgram.programId
    );

    [tickUpperAState, tickUpperAStateBump] = await PublicKey.findProgramAddress([
      TICK_SEED,
      token0.publicKey.toBuffer(),
      token1.publicKey.toBuffer(),
      u32ToSeed(fee),
      u32ToSeed(tickUpper)
    ],
      coreProgram.programId
    );
    [tickUpperBState, tickUpperBStateBump] = await PublicKey.findProgramAddress([
      TICK_SEED,
      token1.publicKey.toBuffer(),
      token2.publicKey.toBuffer(),
      u32ToSeed(fee),
      u32ToSeed(tickUpper)
    ],
      coreProgram.programId
    );

    [bitmapLowerAState, bitmapLowerABump] = await PublicKey.findProgramAddress([
      BITMAP_SEED,
      token0.publicKey.toBuffer(),
      token1.publicKey.toBuffer(),
      u32ToSeed(fee),
      u16ToSeed(wordPosLower),
    ],
      coreProgram.programId
    );
    [bitmapUpperAState, bitmapUpperABump] = await PublicKey.findProgramAddress([
      BITMAP_SEED,
      token0.publicKey.toBuffer(),
      token1.publicKey.toBuffer(),
      u32ToSeed(fee),
      u16ToSeed(wordPosUpper),
    ],
      coreProgram.programId
    );

    [bitmapLowerBState, bitmapLowerBBump] = await PublicKey.findProgramAddress([
      BITMAP_SEED,
      token1.publicKey.toBuffer(),
      token2.publicKey.toBuffer(),
      u32ToSeed(fee),
      u16ToSeed(wordPosLower),
    ],
      coreProgram.programId
    );
    [bitmapUpperBState, bitmapUpperBBump] = await PublicKey.findProgramAddress([
      BITMAP_SEED,
      token1.publicKey.toBuffer(),
      token2.publicKey.toBuffer(),
      u32ToSeed(fee),
      u16ToSeed(wordPosUpper),
    ],
      coreProgram.programId
    );

    [corePositionAState, corePositionABump] = await PublicKey.findProgramAddress([
      POSITION_SEED,
      token0.publicKey.toBuffer(),
      token1.publicKey.toBuffer(),
      u32ToSeed(fee),
      factoryState.toBuffer(),
      u32ToSeed(tickLower),
      u32ToSeed(tickUpper)
    ],
      coreProgram.programId
    );
    [corePositionBState, corePositionBBump] = await PublicKey.findProgramAddress([
      POSITION_SEED,
      token1.publicKey.toBuffer(),
      token2.publicKey.toBuffer(),
      u32ToSeed(fee),
      factoryState.toBuffer(),
      u32ToSeed(tickLower),
      u32ToSeed(tickUpper)
    ],
      coreProgram.programId
    );


    positionANftAccount = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      nftMintAKeypair.publicKey,
      owner,
    )
    positionBNftAccount = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      nftMintBKeypair.publicKey,
      owner,
    )

    const nftMint = new Token(
      connection,
      nftMintAKeypair.publicKey,
      TOKEN_PROGRAM_ID,
      mintAuthority
    )

    metadataAccount = (
      await web3.PublicKey.findProgramAddress(
        [
          Buffer.from('metadata'),
          metaplex.programs.metadata.MetadataProgram.PUBKEY.toBuffer(),
          nftMintAKeypair.publicKey.toBuffer(),
        ],
        metaplex.programs.metadata.MetadataProgram.PUBKEY,
      )
    )[0];

    [tokenizedPositionAState, tokenizedPositionABump] = await PublicKey.findProgramAddress([
      POSITION_SEED,
      nftMintAKeypair.publicKey.toBuffer()
    ],
      coreProgram.programId
    );
    [tokenizedPositionBState, tokenizedPositionBBump] = await PublicKey.findProgramAddress([
      POSITION_SEED,
      nftMintBKeypair.publicKey.toBuffer()
    ],
      coreProgram.programId
    );
  })

  describe('#init_tick_account', () => {
    it('fails if tick is lower than limit', async () => {
      const [invalidLowTickState, invalidLowTickBump] = await PublicKey.findProgramAddress([
        TICK_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        u32ToSeed(MIN_TICK - 1)
      ],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.initTickAccount(MIN_TICK - 1, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          tickState: invalidLowTickState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if tick is higher than limit', async () => {
      const [invalidUpperTickState, invalidUpperTickBump] = await PublicKey.findProgramAddress([
        TICK_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        u32ToSeed(MAX_TICK + 1)
      ],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.initTickAccount(MAX_TICK + 1, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          tickState: invalidUpperTickState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if tick is not a multiple of tick spacing', async () => {
      const invalidTick = 5
      const [tickState, tickBump] = await PublicKey.findProgramAddress([
        TICK_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        u32ToSeed(invalidTick)
      ],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.initTickAccount(invalidTick, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          tickState: tickState,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('creates new tick accounts for lower and upper ticks', async () => {
      await coreProgram.rpc.initTickAccount(tickLower, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          tickState: tickLowerAState,
          systemProgram: SystemProgram.programId,
        }
      })

      await coreProgram.rpc.initTickAccount(tickUpper, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          tickState: tickUpperAState,
          systemProgram: SystemProgram.programId,
        }
      })

      const tickStateLowerData = await coreProgram.account.tickState.fetch(tickLowerAState)
      assert.equal(tickStateLowerData.bump, tickLowerAStateBump)
      assert.equal(tickStateLowerData.tick, tickLower)

      const tickStateUpperData = await coreProgram.account.tickState.fetch(tickUpperAState)
      assert.equal(tickStateUpperData.bump, tickUpperAStateBump)
      assert.equal(tickStateUpperData.tick, tickUpper)
    })
  })

  describe('#init_bitmap_account', () => {
    const minWordPos = (MIN_TICK / tickSpacing) >> 8
    const maxWordPos = (MAX_TICK / tickSpacing) >> 8

    it('fails if tick is lower than limit', async () => {
      const [invalidBitmapLower, invalidBitmapLowerBump] = await PublicKey.findProgramAddress([
        BITMAP_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        u16ToSeed(minWordPos - 1),
      ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.initBitmapAccount(minWordPos - 1, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          bitmapState: invalidBitmapLower,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('fails if tick is higher than limit', async () => {
      const [invalidBitmapUpper, invalidBitmapUpperBump] = await PublicKey.findProgramAddress([
        BITMAP_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        u16ToSeed(maxWordPos + 1),
      ],
        coreProgram.programId
      )

      await expect(coreProgram.rpc.initBitmapAccount(maxWordPos + 1, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          bitmapState: invalidBitmapUpper,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('creates new bitmap account for lower and upper ticks', async () => {
      await coreProgram.rpc.initBitmapAccount(wordPosLower, {
        accounts: {
          signer: owner,
          poolState: poolAState,
          bitmapState: bitmapLowerAState,
          systemProgram: SystemProgram.programId,
        }
      })

      const bitmapLowerData = await coreProgram.account.tickBitmapState.fetch(bitmapLowerAState)
      assert.equal(bitmapLowerData.bump, bitmapLowerABump)
      assert.equal(bitmapLowerData.wordPos, wordPosLower)

      // bitmap upper = bitmap lower
    })
  })

  describe('#init_position_account', () => {
    it('fails if tick lower is not less than tick upper', async () => {
      const [invalidPosition, invalidPositionBump] = await PublicKey.findProgramAddress([
        POSITION_SEED,
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
        factoryState.toBuffer(),
        // posMgrState.toBuffer(),
        u32ToSeed(tickUpper), // upper first
        u32ToSeed(tickLower),
      ],
        coreProgram.programId
      );

      await expect(coreProgram.rpc.initPositionAccount({
        accounts: {
          signer: owner,
          recipient: factoryState,
          poolState: poolAState,
          tickLowerState: tickUpperAState,
          tickUpperState: tickLowerAState,
          positionState: invalidPosition,
          systemProgram: SystemProgram.programId,
        }
      })).to.be.rejectedWith(Error)
    })

    it('creates a new position account', async () => {
      await coreProgram.rpc.initPositionAccount({
        accounts: {
          signer: owner,
          recipient: factoryState,
          poolState: poolAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          positionState: corePositionAState,
          systemProgram: SystemProgram.programId,
        }
      })

      const corePositionData = await coreProgram.account.positionState.fetch(corePositionAState)
      assert.equal(corePositionData.bump, corePositionABump)
    })
  })

  describe('#mint_tokenized_position', () => {

    it('generate observation PDAs', async () => {
      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      lastObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationIndex + 1) % observationCardinalityNext)
        ],
        coreProgram.programId
      ))[0]

    })

    it('fails if past deadline', async () => {
      // connection.slot
      const deadline = new BN(Date.now() / 1000 - 10_000)

      await expect(coreProgram.rpc.mintTokenizedPosition(amount0Desired,
        amount1Desired,
        amount0Minimum,
        amount1Minimum,
        deadline, {
        accounts: {
          minter: owner,
          recipient: owner,
          factoryState,
          nftMint: nftMintAKeypair.publicKey,
          nftAccount: positionANftAccount,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          tokenAccount0: minterWallet0,
          tokenAccount1: minterWallet1,
          vault0: vaultA0,
          vault1: vaultA1,
          lastObservationState: lastObservationAState,
          tokenizedPositionState: tokenizedPositionAState,
          coreProgram: coreProgram.programId,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
        signers: [nftMintAKeypair],
      })).to.be.rejectedWith(Error)
    })

    it('mint tokenized', async () => {
      console.log('minting tokenized position')
      const deadline = new BN(Date.now() / 1000 + 10_000)

      console.log('word upper', wordPosUpper)
      console.log('word upper bytes', u16ToSeed(wordPosUpper))
      await coreProgram.rpc.mintTokenizedPosition(amount0Desired,
        amount1Desired,
        amount0Minimum,
        amount1Minimum,
        deadline, {
        accounts: {
          minter: owner,
          recipient: owner,
          factoryState,
          nftMint: nftMintAKeypair.publicKey,
          nftAccount: positionANftAccount,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          tokenAccount0: minterWallet0,
          tokenAccount1: minterWallet1,
          vault0: vaultA0,
          vault1: vaultA1,
          lastObservationState: lastObservationAState,
          tokenizedPositionState: tokenizedPositionAState,
          coreProgram: coreProgram.programId,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
        signers: [nftMintAKeypair],
      })

      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = coreProgram.addEventListener("IncreaseLiquidityEvent", (event, slot) => {
      //     assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
      //     assert((event.amount0 as BN).eqn(0))
      //     assert((event.amount1 as BN).eq(amount1Desired))

      //     resolve([event, slot]);
      //   });

      //   coreProgram.rpc.mintTokenizedPosition(tokenizedPositionABump,
      //     amount0Desired,
      //     amount1Desired,
      //     amount0Minimum,
      //     amount1Minimum,
      //     deadline, {
      //     accounts: {
      //       minter: owner,
      //       recipient: owner,
      //       factoryState,
      //       nftMint: nftMintAKeypair.publicKey,
      //       nftAccount: positionANftAccount,
      //       poolState: poolAState,
      //       corePositionState: corePositionAState,
      //       tickLowerState: tickLowerAState,
      //       tickUpperState: tickUpperAState,
      //       bitmapLowerState: bitmapLowerAState,
      //       bitmapUpperState: bitmapUpperAState,
      //       tokenAccount0: minterWallet0,
      //       tokenAccount1: minterWallet1,
      //       vault0: vaultA0,
      //       vault1: vaultA1,
      //       lastObservationState: latestObservationAState,
      //       nextObservationState: nextObservationAState,
      //       tokenizedPositionState: tokenizedPositionAState,
      //       coreProgram: coreProgram.programId,
      //       systemProgram: SystemProgram.programId,
      //       rent: web3.SYSVAR_RENT_PUBKEY,
      //       tokenProgram: TOKEN_PROGRAM_ID,
      //       associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID
      //     }, signers: [nftMintAKeypair],
      //   })
      // })
      // await coreProgram.removeEventListener(listener)

      const nftMint = new Token(
        connection,
        nftMintAKeypair.publicKey,
        TOKEN_PROGRAM_ID,
        new Keypair()
      )
      const nftMintInfo = await nftMint.getMintInfo()
      assert.equal(nftMintInfo.decimals, 0)
      const nftAccountInfo = await nftMint.getAccountInfo(positionANftAccount)
      console.log('NFT account info', nftAccountInfo)
      assert(nftAccountInfo.amount.eqn(1))

      const tokenizedPositionData = await coreProgram.account.tokenizedPositionState.fetch(tokenizedPositionAState)
      console.log('Tokenized position', tokenizedPositionData)
      console.log('liquidity inside position', tokenizedPositionData.liquidity.toNumber())
      assert.equal(tokenizedPositionData.bump, tokenizedPositionABump)
      assert(tokenizedPositionData.poolId.equals(poolAState))
      assert(tokenizedPositionData.mint.equals(nftMintAKeypair.publicKey))
      assert.equal(tokenizedPositionData.tickLower, tickLower)
      assert.equal(tokenizedPositionData.tickUpper, tickUpper)
      assert(tokenizedPositionData.feeGrowthInside0LastX32.eqn(0))
      assert(tokenizedPositionData.feeGrowthInside1LastX32.eqn(0))
      assert(tokenizedPositionData.tokensOwed0.eqn(0))
      assert(tokenizedPositionData.tokensOwed1.eqn(0))

      const vault0State = await token0.getAccountInfo(vaultA0)
      // assert(vault0State.amount.eqn(0))
      const vault1State = await token1.getAccountInfo(vaultA1)
      // assert(vault1State.amount.eqn(1_000_000))

      const tickLowerData = await coreProgram.account.tickState.fetch(tickLowerAState)
      console.log('Tick lower', tickLowerData)
      const tickUpperData = await coreProgram.account.tickState.fetch(tickUpperAState)
      console.log('Tick upper', tickUpperData)

      // check if ticks are correctly initialized on the bitmap
      const tickLowerBitmapData = await coreProgram.account.tickBitmapState.fetch(bitmapLowerAState)
      const bitPosLower = (tickLower / tickSpacing) % 256
      const bitPosUpper = (tickUpper / tickSpacing) % 256

      // TODO fix expected calculation
      // const expectedBitmap = [3, 2, 1, 0].map(x => {
      //   let word = new BN(0)
      //   if (bitPosLower >= x * 64) {
      //     const newWord = new BN(1).shln(bitPosLower - x * 64)
      //     word = word.add(newWord)
      //   }
      //   if (bitPosUpper >= x * 64) {
      //     word = word.setn(bitPosUpper - x * 64)
      //     const newWord = new BN(1).shln(bitPosUpper - x * 64)
      //     word = word.add(newWord)
      //   }
      //   return word
      // }).reverse()
      // console.log('expected bitmap', expectedBitmap)
      // console.log('actual bitmap', tickLowerBitmapData.word.map(bn => bn.toString()))
      // for (let i = 0; i < 4; i++) {
      //   assert(tickLowerBitmapData.word[i].eq(expectedBitmap[i]))
      // }

      // const corePositionData = await coreProgram.account.positionState.fetch(corePositionAState)
      // console.log('Core position data', corePositionData)

      // TODO test remaining fields later
      // Look at uniswap tests for reference
    })
  })

  const nftMint = new Token(
    connection,
    nftMintAKeypair.publicKey,
    TOKEN_PROGRAM_ID,
    notOwner
  )

  describe('#add_metaplex_metadata', () => {
    it('Add metadata to a generated position', async () => {
      await coreProgram.rpc.addMetaplexMetadata({
        accounts: {
          payer: owner,
          factoryState,
          nftMint: nftMintAKeypair.publicKey,
          tokenizedPositionState: tokenizedPositionAState,
          metadataAccount,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          metadataProgram: metaplex.programs.metadata.MetadataProgram.PUBKEY,
        }
      })

      const nftMintInfo = await nftMint.getMintInfo()
      assert.isNull(nftMintInfo.mintAuthority)
      const metadata = await Metadata.load(connection, metadataAccount)
      assert.equal(metadata.data.mint, nftMint.publicKey.toString())
      assert.equal(metadata.data.updateAuthority, factoryState.toString())
      assert.equal(metadata.data.data.name, 'Cyclos Positions NFT-V1')
      assert.equal(metadata.data.data.symbol, 'CYS-POS')
      assert.equal(metadata.data.data.uri, 'https://asia-south1-cyclos-finance.cloudfunctions.net/nft?mint=' + nftMint.publicKey.toString())
      assert.deepEqual(metadata.data.data.creators, [{
        address: factoryState.toString(),
        // @ts-ignore
        verified: 1,
        share: 100,
      }])
      assert.equal(metadata.data.data.sellerFeeBasisPoints, 0)
      // @ts-ignore
      assert.equal(metadata.data.isMutable, 0)
    })

    it('fails if metadata is already set', async () => {
      await expect(coreProgram.rpc.addMetaplexMetadata({
        accounts: {
          payer: owner,
          factoryState,
          nftMint: nftMintAKeypair.publicKey,
          tokenizedPositionState: tokenizedPositionAState,
          metadataAccount,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          metadataProgram: metaplex.programs.metadata.MetadataProgram.PUBKEY,
        }
      })).to.be.rejectedWith(Error)
    })
  })

  describe('#increase_liquidity', () => {
    it('fails if past deadline', async () => {
      const deadline = new BN(Date.now() / 1000 - 100_000)
      await expect(coreProgram.rpc.increaseLiquidity(
        amount0Desired,
        amount1Desired,
        amount0Minimum,
        amount1Minimum,
        deadline, {
        accounts: {
          payer: owner,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          tokenAccount0: minterWallet0,
          tokenAccount1: minterWallet1,
          vault0: vaultA0,
          vault1: vaultA1,
          lastObservationState: lastObservationAState,
          tokenizedPositionState: tokenizedPositionAState,
          coreProgram: coreProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )).to.be.rejectedWith(Error)
    })

    it('update observation accounts', async () => {
      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      const { blockTimestamp: lastBlockTime } = await coreProgram.account.observationState.fetch(lastObservationAState)

      const slot = await connection.getSlot()
      const blockTimestamp = await connection.getBlockTime(slot)

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) && lastBlockTime % 14 >= 11) {
        await new Promise(r => setTimeout(r, 3000))
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed(observationIndex)
          ],
          coreProgram.programId
        ))[0]

        nextObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext)
          ],
          coreProgram.programId
        ))[0]
      }

    })

    it('Add token 1 to the position', async () => {
      const deadline = new BN(Date.now() / 1000 + 10_000)

      await coreProgram.rpc.increaseLiquidity(
        amount0Desired,
        amount1Desired,
        amount0Minimum,
        amount1Minimum,
        deadline, {
        accounts: {
          payer: owner,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          tokenAccount0: minterWallet0,
          tokenAccount1: minterWallet1,
          vault0: vaultA0,
          vault1: vaultA1,
          lastObservationState: lastObservationAState,
          tokenizedPositionState: tokenizedPositionAState,
          coreProgram: coreProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      })

      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = coreProgram.addEventListener("IncreaseLiquidityEvent", (event, slot) => {
      //     assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
      //     assert((event.amount0 as BN).eqn(0))
      //     assert((event.amount1 as BN).eq(amount1Desired))

      //     resolve([event, slot]);
      //   });

      //   coreProgram.rpc.increaseLiquidity(
      //     amount0Desired,
      //     amount1Desired,
      //     amount0Minimum,
      //     amount1Minimum,
      //     deadline, {
      //     accounts: {
      //       payer: owner,
      //       factoryState,
      //       poolState: poolAState,
      //       corePositionState: corePositionAState,
      //       tickLowerState: tickLowerAState,
      //       tickUpperState: tickUpperAState,
      //       bitmapLowerState: bitmapLowerAState,
      //       bitmapUpperState: bitmapUpperAState,
      //       tokenAccount0: minterWallet0,
      //       tokenAccount1: minterWallet1,
      //       vault0: vaultA0,
      //       vault1: vaultA1,
      //       lastObservationState: latestObservationAState,
      //       nextObservationState: nextObservationAState,
      //       tokenizedPositionState: tokenizedPositionAState,
      //       coreProgram: coreProgram.programId,
      //       tokenProgram: TOKEN_PROGRAM_ID,
      //     },
      //   }
      //   )
      // })
      // await coreProgram.removeEventListener(listener)

      // const vault0State = await token0.getAccountInfo(vaultA0)
      // assert(vault0State.amount.eqn(0))
      // const vault1State = await token1.getAccountInfo(vaultA1)
      // assert(vault1State.amount.eqn(2_000_000))

      // TODO test remaining fields later
      // Look at uniswap tests for reference
    })

    // To check slippage, we must add liquidity in a price range around
    // current price
  })

  describe('#decrease_liquidity', () => {
    const liquidity = new BN(1999599283)
    const amount1Desired = new BN(999999)

    it('update observation accounts', async () => {
      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      const { blockTimestamp: lastBlockTime } = await coreProgram.account.observationState.fetch(lastObservationAState)

      const slot = await connection.getSlot()
      const blockTimestamp = await connection.getBlockTime(slot)

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) && lastBlockTime % 14 >= 11) {
        await new Promise(r => setTimeout(r, 3000))
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed(observationIndex)
          ],
          coreProgram.programId
        ))[0]

        nextObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext)
          ],
          coreProgram.programId
        ))[0]
      }
    })

    it('fails if past deadline', async () => {
      const deadline = new BN(Date.now() / 1000 - 100_000)
      await expect(coreProgram.rpc.decreaseLiquidity(
        liquidity,
        new BN(0),
        amount1Desired,
        deadline, {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )).to.be.rejectedWith(Error)
    })

    it('fails if not called by the owner', async () => {
      const deadline = new BN(Date.now() / 1000 + 10_000)
      await expect(coreProgram.rpc.decreaseLiquidity(
        liquidity,
        new BN(0),
        amount1Desired,
        deadline, {
        accounts: {
          ownerOrDelegate: notOwner,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )).to.be.rejectedWith(Error)
    })

    it('fails if past slippage tolerance', async () => {
      const deadline = new BN(Date.now() / 1000 + 10_000)
      await expect(coreProgram.rpc.decreaseLiquidity(
        liquidity,
        new BN(0),
        new BN(1_000_000), // 999_999 available
        deadline, {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )).to.be.rejectedWith(Error)
    })

    it('generate a temporary NFT account for testing', async () => {
      temporaryNftHolder = await nftMint.createAssociatedTokenAccount(mintAuthority.publicKey)
    })

    it('fails if NFT token account for the user is empty', async () => {
      const transferTx = new web3.Transaction()
      transferTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      transferTx.add(Token.createTransferInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        temporaryNftHolder,
        owner,
        [],
        1
      ))

      await anchor.getProvider().send(transferTx)

      const deadline = new BN(Date.now() / 1000 + 10_000)
      await expect(coreProgram.rpc.decreaseLiquidity(
        liquidity,
        new BN(0),
        amount1Desired,
        deadline, {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount, // no balance
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )).to.be.rejectedWith(Error)

      // send the NFT back to the original owner
      await nftMint.transfer(
        temporaryNftHolder,
        positionANftAccount,
        mintAuthority,
        [],
        1
      )
    })

    it('burn half of the position liquidity as owner', async () => {
      const deadline = new BN(Date.now() / 1000 + 10_000)

      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("DecreaseLiquidityEvent", (event, slot) => {
          assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
          assert((event.liquidity as BN).eq(liquidity))
          assert((event.amount0 as BN).eqn(0))
          assert((event.amount1 as BN).eq(amount1Desired))

          resolve([event, slot]);
        });

        coreProgram.rpc.decreaseLiquidity(
          liquidity,
          new BN(0),
          amount1Desired,
          deadline, {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            tokenizedPositionState: tokenizedPositionAState,
            factoryState,
            poolState: poolAState,
            corePositionState: corePositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            coreProgram: coreProgram.programId
          },
          remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          }],
        }
        )
      })
      await coreProgram.removeEventListener(listener)
      const tokenizedPositionData = await coreProgram.account.tokenizedPositionState.fetch(tokenizedPositionAState)
      assert(tokenizedPositionData.tokensOwed0.eqn(0))
      assert(tokenizedPositionData.tokensOwed1.eqn(999999))
    })

    it('fails if 0 tokens are delegated', async () => {
      const approveTx = new web3.Transaction()
      approveTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      approveTx.add(Token.createApproveInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        mintAuthority.publicKey,
        owner,
        [],
        0
      ))
      await anchor.getProvider().send(approveTx)

      const deadline = new BN(Date.now() / 1000 + 10_000)
      const tx = coreProgram.transaction.decreaseLiquidity(
        new BN(1_000),
        new BN(0),
        new BN(0),
        deadline, {
        accounts: {
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )
      await expect(connection.sendTransaction(tx, [mintAuthority])).to.be.rejectedWith(Error)
      // TODO see why errors inside functions are not propagating outside
    })

    it('burn liquidity as the delegated authority', async () => {
      const approveTx = new web3.Transaction()
      approveTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      approveTx.add(Token.createApproveInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        mintAuthority.publicKey,
        owner,
        [],
        1
      ))
      await anchor.getProvider().send(approveTx)

      const deadline = new BN(Date.now() / 1000 + 10_000)
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("DecreaseLiquidityEvent", (event, slot) => {
          resolve([event, slot]);
        });

        const tx = coreProgram.transaction.decreaseLiquidity(
          new BN(1_000_000),
          new BN(0),
          new BN(0),
          deadline, {
          accounts: {
            ownerOrDelegate: mintAuthority.publicKey,
            nftAccount: positionANftAccount,
            tokenizedPositionState: tokenizedPositionAState,
            factoryState,
            poolState: poolAState,
            corePositionState: corePositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            coreProgram: coreProgram.programId
          },
          remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          }],
        }
        )
        connection.sendTransaction(tx, [mintAuthority])
      })
      await coreProgram.removeEventListener(listener)
    })

    it('fails if delegation is revoked', async () => {
      const revokeTx = new web3.Transaction()
      revokeTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      revokeTx.add(Token.createRevokeInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        owner,
        [],
      ))
      await anchor.getProvider().send(revokeTx)

      const deadline = new BN(Date.now() / 1000 + 10_000)
      const tx = coreProgram.transaction.decreaseLiquidity(
        new BN(1_000_000),
        new BN(0),
        new BN(0),
        deadline, {
        accounts: {
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      }
      )
      // TODO check for 'Not approved' error
      await expect(connection.sendTransaction(tx, [mintAuthority])).to.be.rejectedWith(Error)
    })
  })

  describe('#collect', () => {
    it('fails if both amounts are set as 0', async () => {
      await expect(coreProgram.rpc.collectFromTokenized(new BN(0), new BN(0), {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId,
          vault0: vaultA0,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      })).to.be.rejectedWith(Error)
    })

    it('fails if signer is not the owner or a delegated authority', async () => {
      const tx = coreProgram.transaction.collectFromTokenized(new BN(0), new BN(10), {
        accounts: {
          ownerOrDelegate: notOwner.publicKey,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId,
          vault0: vaultA0,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      })
      await expect(connection.sendTransaction(tx, [notOwner])).to.be.rejectedWith(Error)
    })

    it('fails delegated amount is 0', async () => {
      const approveTx = new web3.Transaction()
      approveTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      approveTx.add(Token.createApproveInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        mintAuthority.publicKey,
        owner,
        [],
        0
      ))
      await anchor.getProvider().send(approveTx)

      const tx = coreProgram.transaction.collectFromTokenized(new BN(0), new BN(10), {
        accounts: {
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId,
          vault0: vaultA0,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      })
      await expect(connection.sendTransaction(tx, [mintAuthority])).to.be.rejectedWith(Error)
    })

    it('fails if NFT token account is empty', async () => {
      const transferTx = new web3.Transaction()
      transferTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      transferTx.add(Token.createTransferInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        temporaryNftHolder,
        owner,
        [],
        1
      ))
      await anchor.getProvider().send(transferTx)

      await expect(coreProgram.rpc.collectFromTokenized(new BN(0), new BN(10), {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          tokenizedPositionState: tokenizedPositionAState,
          factoryState,
          poolState: poolAState,
          corePositionState: corePositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          coreProgram: coreProgram.programId,
          vault0: vaultA0,
          vault1: vaultA1,
          recipientWallet0: feeRecipientWallet0,
          recipientWallet1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [{
          pubkey: nextObservationAState,
          isSigner: false,
          isWritable: true
        }],
      })).to.be.rejectedWith(Error)

      // send the NFT back to the original owner
      await nftMint.transfer(
        temporaryNftHolder,
        positionANftAccount,
        mintAuthority,
        [],
        1
      )
    })

    it('collect a portion of owed tokens as owner', async () => {
      const amount0Max = new BN(0)
      const amount1Max = new BN(10)
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("CollectTokenizedEvent", (event, slot) => {
          assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
          assert((event.amount0 as BN).eq(amount0Max))
          assert((event.amount1 as BN).eq(amount1Max))
          assert((event.recipientWallet0 as web3.PublicKey).equals(feeRecipientWallet0))
          assert((event.recipientWallet1 as web3.PublicKey).equals(feeRecipientWallet1))

          resolve([event, slot]);
        });

        coreProgram.rpc.collectFromTokenized(amount0Max, amount1Max, {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            tokenizedPositionState: tokenizedPositionAState,
            factoryState,
            poolState: poolAState,
            corePositionState: corePositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            coreProgram: coreProgram.programId,
            vault0: vaultA0,
            vault1: vaultA1,
            recipientWallet0: feeRecipientWallet0,
            recipientWallet1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          }],
        })
      })
      await coreProgram.removeEventListener(listener)

      const corePositionData = await coreProgram.account.positionState.fetch(corePositionAState)
      assert(corePositionData.tokensOwed0.eqn(0))
      assert(corePositionData.tokensOwed1.eqn(1000489)) // minus 10

      const tokenizedPositionData = await coreProgram.account.tokenizedPositionState.fetch(tokenizedPositionAState)
      assert(tokenizedPositionData.tokensOwed0.eqn(0))
      assert(tokenizedPositionData.tokensOwed1.eqn(1000489))

      const recipientWallet0Info = await token0.getAccountInfo(feeRecipientWallet0)
      const recipientWallet1Info = await token1.getAccountInfo(feeRecipientWallet1)
      assert(recipientWallet0Info.amount.eqn(0))
      assert(recipientWallet1Info.amount.eqn(10))

      const vault0Info = await token0.getAccountInfo(vaultA0)
      const vault1Info = await token1.getAccountInfo(vaultA1)
      assert(vault0Info.amount.eqn(0))
      assert(vault1Info.amount.eqn(1999990)) // minus 10
    })

    it('collect a portion of owed tokens as the delegated authority', async () => {
      const approveTx = new web3.Transaction()
      approveTx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      approveTx.add(Token.createApproveInstruction(
        TOKEN_PROGRAM_ID,
        positionANftAccount,
        mintAuthority.publicKey,
        owner,
        [],
        1
      ))
      await anchor.getProvider().send(approveTx)

      const amount0Max = new BN(0)
      const amount1Max = new BN(10)
      let listener: number
      let [_event, _slot] = await new Promise((resolve, _reject) => {
        listener = coreProgram.addEventListener("CollectTokenizedEvent", (event, slot) => {
          assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
          assert((event.amount0 as BN).eq(amount0Max))
          assert((event.amount1 as BN).eq(amount1Max))
          assert((event.recipientWallet0 as web3.PublicKey).equals(feeRecipientWallet0))
          assert((event.recipientWallet1 as web3.PublicKey).equals(feeRecipientWallet1))

          resolve([event, slot]);
        });

        const tx = coreProgram.transaction.collectFromTokenized(new BN(0), new BN(10), {
          accounts: {
            ownerOrDelegate: mintAuthority.publicKey,
            nftAccount: positionANftAccount,
            tokenizedPositionState: tokenizedPositionAState,
            factoryState,
            poolState: poolAState,
            corePositionState: corePositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            coreProgram: coreProgram.programId,
            vault0: vaultA0,
            vault1: vaultA1,
            recipientWallet0: feeRecipientWallet0,
            recipientWallet1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          }],
        })
        connection.sendTransaction(tx, [mintAuthority])
      })
      await coreProgram.removeEventListener(listener)

      const corePositionData = await coreProgram.account.positionState.fetch(corePositionAState)
      assert(corePositionData.tokensOwed0.eqn(0))
      assert(corePositionData.tokensOwed1.eqn(1000479))

      const tokenizedPositionData = await coreProgram.account.tokenizedPositionState.fetch(tokenizedPositionAState)
      assert(tokenizedPositionData.tokensOwed0.eqn(0))
      assert(tokenizedPositionData.tokensOwed1.eqn(1000479))

      const recipientWallet0Info = await token0.getAccountInfo(feeRecipientWallet0)
      const recipientWallet1Info = await token1.getAccountInfo(feeRecipientWallet1)
      assert(recipientWallet0Info.amount.eqn(0))
      assert(recipientWallet1Info.amount.eqn(20))

      const vault0Info = await token0.getAccountInfo(vaultA0)
      const vault1Info = await token1.getAccountInfo(vaultA1)
      assert(vault0Info.amount.eqn(0))
      assert(vault1Info.amount.eqn(1999980))
    })
  })

  describe('#exact_input_single', () => {
    // before swapping, current tick = 10 and price = 4297115210
    // active ticks are 0 and 10
    // entire liquidity is in token_1

    const deadline = new BN(Date.now() / 1000 + 1_000_000)

    it('fails if limit price is greater than current pool price', async () => {
      const amountIn = new BN(100_000)
      const amountOutMinimum = new BN(0)
      const sqrtPriceLimitX32 = new BN(4297115220)

      await expect(coreProgram.rpc.exactInputSingle(
        deadline,
        // true,
        amountIn,
        amountOutMinimum,
        sqrtPriceLimitX32,
        {
          accounts: {
            signer: owner,
            factoryState,
            poolState: poolAState,
            inputTokenAccount: minterWallet0,
            outputTokenAccount: minterWallet1,
            inputVault: vaultA0,
            outputVault: vaultA1,
            lastObservationState: lastObservationAState,
            // nextObservationState: nextObservationAState,
            coreProgram: coreProgram.programId,
            tokenProgram: TOKEN_PROGRAM_ID,
          }, remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          },{
            pubkey: bitmapLowerAState,
            isSigner: false,
            isWritable: true
          },
          // price moves downwards in zero for one swap
          {
            pubkey: tickUpperAState,
            isSigner: false,
            isWritable: true
          }, {
            pubkey: tickLowerAState,
            isSigner: false,
            isWritable: true
          }]
        }
      )).to.be.rejectedWith(Error)
    })

    it('swap upto a limit price for a zero to one swap', async () => {
      const amountIn = new BN(100_000)
      const amountOutMinimum = new BN(0)
      const sqrtPriceLimitX32 = new BN(4297115200) // current price is 4297115210

      const tickDataProvider = new SolanaTickDataProvider(coreProgram, {
        token0: token0.publicKey,
        token1: token1.publicKey,
        fee,
      })

      const { tick: currentTick, sqrtPriceX32: currentSqrtPriceX32, liquidity: currentLiquidity } = await coreProgram.account.poolState.fetch(poolAState)
      await tickDataProvider.eagerLoadCache(currentTick, tickSpacing)
      // output is one tick behind actual (8 instead of 9)
      uniPoolA = new Pool(
        uniToken0,
        uniToken1,
        fee,
        JSBI.BigInt(currentSqrtPriceX32),
        JSBI.BigInt(currentLiquidity),
        currentTick,
        tickDataProvider
      )

      const [expectedAmountOut, expectedNewPool, bitmapAndTickAccounts] = await uniPoolA.getOutputAmount(
        CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber()),
        JSBI.BigInt(sqrtPriceLimitX32),
      )
      assert.equal(expectedNewPool.sqrtRatioX32.toString(), sqrtPriceLimitX32.toString())

      await coreProgram.rpc.exactInputSingle(
        deadline,
        amountIn,
        amountOutMinimum,
        sqrtPriceLimitX32,
        {
          accounts: {
            signer: owner,
            factoryState,
            poolState: poolAState,
            inputTokenAccount: minterWallet0,
            outputTokenAccount: minterWallet1,
            inputVault: vaultA0,
            outputVault: vaultA1,
            lastObservationState: lastObservationAState,
            // nextObservationState: nextObservationAState,
            coreProgram: coreProgram.programId,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            ...bitmapAndTickAccounts,
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true
            },
          ],
        }
      )
      let poolStateData = await coreProgram.account.poolState.fetch(poolAState)
      assert(poolStateData.sqrtPriceX32.eq(sqrtPriceLimitX32))

      console.log('tick after swap', poolStateData.tick, 'price', poolStateData.sqrtPriceX32.toString())
      uniPoolA = expectedNewPool
    })

    it('performs a zero for one swap without a limit price', async () => {
      let poolStateDataBefore = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool price', poolStateDataBefore.sqrtPriceX32.toNumber())
      console.log('pool tick', poolStateDataBefore.tick)

      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      lastObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationIndex + 1) % observationCardinalityNext)
        ],
        coreProgram.programId
      ))[0]

      const amountIn = new BN(100_000)
      const amountOutMinimum = new BN(0)
      const sqrtPriceLimitX32 = new BN(0)

      console.log('pool tick', uniPoolA.tickCurrent, 'price', uniPoolA.sqrtRatioX32.toString())
      const [expectedAmountOut, expectedNewPool, bitmapAndTickAccounts] = await uniPoolA.getOutputAmount(
        CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber())
      )
      console.log('expected pool', expectedNewPool)

      await coreProgram.rpc.exactInputSingle(
        deadline,
        // true,
        amountIn,
        amountOutMinimum,
        sqrtPriceLimitX32,
        {
          accounts: {
            signer: owner,
            factoryState,
            poolState: poolAState,
            inputTokenAccount: minterWallet0,
            outputTokenAccount: minterWallet1,
            inputVault: vaultA0,
            outputVault: vaultA1,
            lastObservationState: lastObservationAState,
            // nextObservationState: nextObservationAState,
            coreProgram: coreProgram.programId,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            ...bitmapAndTickAccounts,
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true
            },
          ],
        }
      )
      const poolStateDataAfter = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool price after', poolStateDataAfter.sqrtPriceX32.toNumber())
      console.log('pool tick after', poolStateDataAfter.tick)

      uniPoolA = expectedNewPool
    })
  })

  describe('#exact_input', () => {

    const deadline = new BN(Date.now() / 1000 + 10_000)
    it('performs a single pool swap', async () => {
      const poolStateDataBefore = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool price', poolStateDataBefore.sqrtPriceX32.toNumber())
      console.log('pool tick', poolStateDataBefore.tick)

      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      lastObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationIndex + 1) % observationCardinalityNext)
        ],
        coreProgram.programId
      ))[0]

      const amountIn = new BN(100_000)
      const amountOutMinimum = new BN(0)
      const [expectedAmountOut, expectedNewPool, swapAccounts] = await uniPoolA.getOutputAmount(
        CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber())
      )
      console.log('expected pool', expectedNewPool)

      await coreProgram.rpc.exactInput(
        deadline,
        amountIn,
        amountOutMinimum,
        Buffer.from([2]),
        {
          accounts: {
            signer: owner,
            factoryState,
            inputTokenAccount: minterWallet0,
            coreProgram: coreProgram.programId,
            tokenProgram: TOKEN_PROGRAM_ID,
          }, remainingAccounts: [{
            pubkey: poolAState,
            isSigner: false,
            isWritable: true
          }, {
            pubkey: minterWallet1, // outputTokenAccount
            isSigner: false,
            isWritable: true
          }, {
            pubkey: vaultA0, // input vault
            isSigner: false,
            isWritable: true
          }, {
            pubkey: vaultA1, // output vault
            isSigner: false,
            isWritable: true
          }, {
            pubkey: lastObservationAState,
            isSigner: false,
            isWritable: true
          },
          ...swapAccounts,
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          },
          ]
        }
      )

      const poolStateDataAfter = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool price after', poolStateDataAfter.sqrtPriceX32.toNumber())
      console.log('pool tick after', poolStateDataAfter.tick)
    })

    it('creates a second liquidity pool', async () => {
      await coreProgram.rpc.createAndInitPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          token0: token1.publicKey,
          token1: token2.publicKey,
          feeState,
          poolState: poolBState,
          initialObservationState: initialObservationStateB,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        }
      })
      console.log('second pool created')

      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolBState)

      latestObservationBState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token1.publicKey.toBuffer(),
          token2.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationBState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token1.publicKey.toBuffer(),
          token2.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationIndex + 1) % observationCardinalityNext)
        ],
        coreProgram.programId
      ))[0]

      // create tick and bitmap accounts
      // can't combine with createTokenizedPosition due to size limit

      const tx = new web3.Transaction()
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      tx.instructions = [
        coreProgram.instruction.initTickAccount(tickLower, {
          accounts: {
            signer: owner,
            poolState: poolBState,
            tickState: tickLowerBState,
            systemProgram: SystemProgram.programId,
          }
        }),
        coreProgram.instruction.initTickAccount(tickUpper, {
          accounts: {
            signer: owner,
            poolState: poolBState,
            tickState: tickUpperBState,
            systemProgram: SystemProgram.programId,
          }
        }),
        coreProgram.instruction.initBitmapAccount(wordPosLower, {
          accounts: {
            signer: owner,
            poolState: poolBState,
            bitmapState: bitmapLowerBState,
            systemProgram: SystemProgram.programId,
          }
        }),
        coreProgram.instruction.initPositionAccount({
          accounts: {
            signer: owner,
            recipient: factoryState,
            poolState: poolBState,
            tickLowerState: tickLowerBState,
            tickUpperState: tickUpperBState,
            positionState: corePositionBState,
            systemProgram: SystemProgram.programId,
          }
        })
      ]
      await anchor.getProvider().send(tx)

      console.log('creating tokenized position')
      await coreProgram.rpc.mintTokenizedPosition(amount0Desired,
        amount1Desired,
        new BN(0),
        new BN(0),
        deadline, {
        accounts: {
          minter: owner,
          recipient: owner,
          factoryState,
          nftMint: nftMintBKeypair.publicKey,
          nftAccount: positionBNftAccount,
          poolState: poolBState,
          corePositionState: corePositionBState,
          tickLowerState: tickLowerBState,
          tickUpperState: tickUpperBState,
          bitmapLowerState: bitmapLowerBState,
          bitmapUpperState: bitmapUpperBState,
          tokenAccount0: minterWallet1,
          tokenAccount1: minterWallet2,
          vault0: vaultB1,
          vault1: vaultB2,
          lastObservationState: latestObservationBState,
          tokenizedPositionState: tokenizedPositionBState,

          coreProgram: coreProgram.programId,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID
        },
        remainingAccounts: [{
          pubkey: nextObservationBState,
          isSigner: false,
          isWritable: true
        }],
        signers: [nftMintBKeypair],
      })
    })

    it('perform a two pool swap', async () => {
      const poolStateDataBefore = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool price', poolStateDataBefore.sqrtPriceX32.toNumber())
      console.log('pool tick', poolStateDataBefore.tick)

      const {
        observationIndex: observationAIndex,
        observationCardinalityNext: observationCardinalityANext
      } = await coreProgram.account.poolState.fetch(poolAState)

      lastObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationAIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationAState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token0.publicKey.toBuffer(),
          token1.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationAIndex + 1) % observationCardinalityANext)
        ],
        coreProgram.programId
      ))[0]

      const {
        observationIndex: observationBIndex,
        observationCardinalityNext: observationCardinalityBNext
      } = await coreProgram.account.poolState.fetch(poolBState)

      latestObservationBState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token1.publicKey.toBuffer(),
          token2.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed(observationBIndex)
        ],
        coreProgram.programId
      ))[0]

      nextObservationBState = (await PublicKey.findProgramAddress(
        [
          OBSERVATION_SEED,
          token1.publicKey.toBuffer(),
          token2.publicKey.toBuffer(),
          u32ToSeed(fee),
          u16ToSeed((observationBIndex + 1) % observationCardinalityBNext)
        ],
        coreProgram.programId
      ))[0]

      let vaultBalanceA0 = await token0.getAccountInfo(vaultA0)
      let vaultBalanceA1 = await token1.getAccountInfo(vaultA1)
      let vaultBalanceB1 = await token1.getAccountInfo(vaultB1)
      let vaultBalanceB2 = await token2.getAccountInfo(vaultB2)
      console.log(
        'vault balances before',
        vaultBalanceA0.amount.toNumber(),
        vaultBalanceA1.amount.toNumber(),
        vaultBalanceB1.amount.toNumber(),
        vaultBalanceB2.amount.toNumber()
      )
      let token2AccountInfo = await token2.getAccountInfo(minterWallet2)
      console.log('token 2 balance before', token2AccountInfo.amount.toNumber())

      console.log('pool B address', poolBState.toString())

      const amountIn = new BN(100_000)
      const amountOutMinimum = new BN(0)
      await coreProgram.rpc.exactInput(
        deadline,
        amountIn,
        amountOutMinimum,
        Buffer.from([2, 2]),
        {
          accounts: {
            signer: owner,
            factoryState,
            inputTokenAccount: minterWallet0,
            coreProgram: coreProgram.programId,
            tokenProgram: TOKEN_PROGRAM_ID,
          }, remainingAccounts: [{
            pubkey: poolAState,
            isSigner: false,
            isWritable: true
          },{
            pubkey: minterWallet1, // outputTokenAccount
            isSigner: false,
            isWritable: true
          },{
            pubkey: vaultA0, // input vault
            isSigner: false,
            isWritable: true
          },{
            pubkey: vaultA1, // output vault
            isSigner: false,
            isWritable: true
          },{
            pubkey: lastObservationAState,
            isSigner: false,
            isWritable: true
          },
          {
            pubkey: bitmapLowerAState,
            isSigner: false,
            isWritable: true
          },
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          },
          // second pool
          {
            pubkey: poolBState,
            isSigner: false,
            isWritable: true
          },{
            pubkey: minterWallet2, // outputTokenAccount
            isSigner: false,
            isWritable: true
          },{
            pubkey: vaultB1, // input vault
            isSigner: false,
            isWritable: true
          },{
            pubkey: vaultB2, // output vault
            isSigner: false,
            isWritable: true
          },{
            pubkey: latestObservationBState,
            isSigner: false,
            isWritable: true
          },
            {
            pubkey: bitmapLowerBState,
            isSigner: false,
            isWritable: true
          }, {
            pubkey: tickUpperBState,
            isSigner: false,
            isWritable: true
          },
          {
            pubkey: nextObservationBState,
            isSigner: false,
            isWritable: true
          },
        ]
        }
      )
      vaultBalanceA0 = await token0.getAccountInfo(vaultA0)
      vaultBalanceA1 = await token1.getAccountInfo(vaultA1)
      vaultBalanceB1 = await token1.getAccountInfo(vaultB1)
      vaultBalanceB2 = await token2.getAccountInfo(vaultB2)
      console.log(
        'vault balances after',
        vaultBalanceA0.amount.toNumber(),
        vaultBalanceA1.amount.toNumber(),
        vaultBalanceB1.amount.toNumber(),
        vaultBalanceB2.amount.toNumber()
      )

      const poolStateDataAfter = await coreProgram.account.poolState.fetch(poolAState)
      console.log('pool A price after', poolStateDataAfter.sqrtPriceX32.toNumber())
      console.log('pool A tick after', poolStateDataAfter.tick)

      token2AccountInfo = await token2.getAccountInfo(minterWallet2)
      console.log('token 2 balance after', token2AccountInfo.amount.toNumber())
    })
  })

  describe('Completely close position and deallocate ticks', () => {
    it('update observation accounts', async () => {
      const {
        observationIndex,
        observationCardinalityNext
      } = await coreProgram.account.poolState.fetch(poolAState)

      const { blockTimestamp: lastBlockTime } = await coreProgram.account.observationState.fetch(lastObservationAState)

      const slot = await connection.getSlot()
      const blockTimestamp = await connection.getBlockTime(slot)

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) && lastBlockTime % 14 >= 11) {
        await new Promise(r => setTimeout(r, 3000))
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed(observationIndex)
          ],
          coreProgram.programId
        ))[0]

        nextObservationAState = (await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            token0.publicKey.toBuffer(),
            token1.publicKey.toBuffer(),
            u32ToSeed(fee),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext)
          ],
          coreProgram.programId
        ))[0]
      }
    })

    it('burn entire of the position liquidity as owner', async () => {
      const { liquidity } = await coreProgram.account.tokenizedPositionState.fetch(tokenizedPositionAState)
      console.log('liquidity in position', liquidity)
      const deadline = new BN(Date.now() / 1000 + 10_000)

      const tx = new Transaction()
      tx.instructions = [
        coreProgram.instruction.decreaseLiquidity(
          liquidity,
          new BN(0),
          new BN(0),
          deadline, {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            tokenizedPositionState: tokenizedPositionAState,
            factoryState,
            poolState: poolAState,
            corePositionState: corePositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            coreProgram: coreProgram.programId
          },
          remainingAccounts: [{
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true
          }],
        }
        ),
        coreProgram.instruction.closeTickAccount({
          accounts: {
            recipient: owner,
            tickState: tickLowerAState,
          }
        }),
        coreProgram.instruction.closeTickAccount({
          accounts: {
            recipient: owner,
            tickState: tickUpperAState,
          }
        })
      ]
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash
      await anchor.getProvider().send(tx)
    })
  })
})
