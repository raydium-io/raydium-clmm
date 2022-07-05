import * as anchor from "@project-serum/anchor";
import {
  Program,
  web3,
  BN,
  ProgramError,
  eventDiscriminator,
} from "@project-serum/anchor";
import * as metaplex from "@metaplex/js";
import {
  Token,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Pool,
  u16ToSeed,
  u32ToSeed,
  TickMath,
  maxLiquidityForAmounts,
} from "@cykura/sdk";
import { CurrencyAmount, Token as UniToken, BigintIsh } from "@cykura/sdk-core";
import { assert, expect } from "chai";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
chai.use(chaiAsPromised);

import { AmmCore } from "../target/types/amm_core";
import {
  MaxU64,
  MAX_SQRT_RATIO,
  MAX_TICK,
  MIN_SQRT_RATIO,
  MIN_TICK,
  BITMAP_SEED,
  FEE_SEED,
  OBSERVATION_SEED,
  POOL_SEED,
  POOL_VAULT_SEED,
  POOL_REWARD_VAULT_SEED,
  POSITION_SEED,
  TICK_SEED,
  accountExist,
  getUnixTs,
  sleep,
  getBlockTimestamp,
} from "./utils";
import SolanaTickDataProvider from "./SolanaTickDataProvider";
import { Transaction, ConfirmOptions } from "@solana/web3.js";
import JSBI from "jsbi";
import { Spl } from "@raydium-io/raydium-sdk";

console.log("starting test");
const {
  metadata: { Metadata },
} = metaplex.programs;

const { PublicKey, Keypair, SystemProgram } = anchor.web3;

describe("amm-core", async () => {
  console.log("in describe");

  // const customConfirmOptions= ConfirmOptions{
  //     skipPreflight: true,
  //     preflightCommitment: "processed",
  //     commitment: "processed",
  // };

  // Configure the client to use the local cluster.
  const provider = anchor.Provider.env();
  provider.opts.skipPreflight = true;
  anchor.setProvider(provider);
  console.log("provider set");

  const program = anchor.workspace.AmmCore as Program<AmmCore>;
  console.log("program created");
  const { connection, wallet } = anchor.getProvider();
  const owner = anchor.getProvider().wallet.publicKey;
  console.log("owner address: ", owner.toString());
  const notOwner = new Keypair();

  const fee = 500; // 500 / 1e6 = 0.0005
  const tickSpacing = 10;

  // find factory address
  const [ammConfig, ammConfigBump] = await PublicKey.findProgramAddress(
    [],
    program.programId
  );
  console.log("factory address: ", ammConfig.toString());

  // find fee address
  const [feeState, feeStateBump] = await PublicKey.findProgramAddress(
    [FEE_SEED, u32ToSeed(fee)],
    program.programId
  );
  console.log("Fee", feeState.toString(), feeStateBump);

  const mintAuthority = new Keypair();

  // serum market
  // const serumMarketA = new Keypair().publicKey;
  // const serumMarketB = new Keypair().publicKey;

  // Tokens constituting the pool
  let token0: Token;
  let token1: Token;
  let token2: Token;

  let uniToken0: UniToken;
  let uniToken1: UniToken;
  let uniToken2: UniToken;

  let uniPoolA: Pool;

  // ATAs to hold pool tokens
  let vaultA0: web3.PublicKey;
  let _bumpA0;
  let vaultA1: web3.PublicKey;
  let _bumpA1;
  let vaultB1: web3.PublicKey;
  let _bumpB1;
  let vaultB2: web3.PublicKey;
  let _bumpB2;

  let rewardFounder = new Keypair();
  // Reward token
  let rewardToken0: Token;
  let rewardToken1: Token;
  let rewardToken2: Token;

  // reward token vault
  let rewardVault0: web3.PublicKey;
  let _reward_bump0;
  let rewardVault1: web3.PublicKey;
  let _reward_bump1;
  let rewardVault2: web3.PublicKey;
  let _reward_bump2;

  let rewardFounderTokenAccount0: web3.PublicKey;
  let rewardFounderTokenAccount1: web3.PublicKey;
  let rewardFounderTokenAccount2: web3.PublicKey;

  let ownerRewardTokenAccount0: web3.PublicKey;
  let ownerRewardTokenAccount1: web3.PublicKey;
  let ownerRewardTokenAccount2: web3.PublicKey;

  let poolAState: web3.PublicKey;
  let poolAStateBump: number;
  let poolBState: web3.PublicKey;
  let poolBStateBump: number;

  let initialObservationStateA: web3.PublicKey;
  let initialObservationBumpA: number;
  let initialObservationStateB: web3.PublicKey;
  let initialObservationBumpB: number;

  // These accounts will spend tokens to mint the position
  let minterWallet0: web3.PublicKey;
  let minterWallet1: web3.PublicKey;
  let minterWallet2: web3.PublicKey;

  let temporaryNftHolder: web3.PublicKey;

  const tickLower = 0;
  const tickUpper = 10;
  const wordPosLower = (tickLower / tickSpacing) >> 8;
  const wordPosUpper = (tickUpper / tickSpacing) >> 8;

  const amount0Desired = new BN(1_000_000);
  const amount1Desired = new BN(1_000_000);
  const amount0Minimum = new BN(0);
  const amount1Minimum = new BN(0);

  const nftMintAKeypair = new Keypair();
  const nftMintBKeypair = new web3.Keypair();

  let tickLowerAState: web3.PublicKey;
  let tickLowerAStateBump: number;
  let tickLowerBState: web3.PublicKey;
  let tickLowerBStateBump: number;
  let tickUpperAState: web3.PublicKey;
  let tickUpperAStateBump: number;
  let tickUpperBState: web3.PublicKey;
  let tickUpperBStateBump: number;
  let protocolPositionAState: web3.PublicKey;
  let corePositionABump: number;
  let protocolPositionBState: web3.PublicKey;
  let corePositionBBump: number;
  let bitmapLowerAState: web3.PublicKey;
  let bitmapLowerABump: number;
  let bitmapLowerBState: web3.PublicKey;
  let bitmapLowerBBump: number;
  let bitmapUpperAState: web3.PublicKey;
  let bitmapUpperABump: number;
  let bitmapUpperBState: web3.PublicKey;
  let bitmapUpperBBump: number;
  let personalPositionAState: web3.PublicKey;
  let personalPositionABump: number;
  let personalPositionBState: web3.PublicKey;
  let personalPositionBBump: number;
  let positionANftAccount: web3.PublicKey;
  let positionBNftAccount: web3.PublicKey;
  let metadataAccount: web3.PublicKey;
  let metadataBAccount: web3.PublicKey;
  let lastObservationAState: web3.PublicKey;
  let nextObservationAState: web3.PublicKey;
  let latestObservationBState: web3.PublicKey;
  let nextObservationBState: web3.PublicKey;

  const protocolFeeRecipient = new Keypair();
  let feeRecipientWallet0: web3.PublicKey;
  let feeRecipientWallet1: web3.PublicKey;
  // 1 << 32 = 4294967296
  // 4297115210 / 4294967296 = 1
  // float(4297115210 % 4294967296) / 4294967296 = 0.0005001001991331577
  // price = 1.0005001001991331577 = √(1.0001^i) = 1.0001^(i/2) ------> i = 10
  const initialPriceX32 = new BN(4297115210);
  const initialTick = 10;

  console.log("before token test");
  it("Create token mints", async () => {
    console.log("creating tokens");
    const transferSolTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: mintAuthority.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    );
    transferSolTx.add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: notOwner.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    );
    transferSolTx.add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: rewardFounder.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    );
    await anchor.getProvider().send(transferSolTx);

    token0 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );
    token1 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );
    token2 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );

    if (token0.publicKey.toString() > token1.publicKey.toString()) {
      // swap token mints
      console.log("Swap tokens for A");
      const temp = token0;
      token0 = token1;
      token1 = temp;
    }

    uniToken0 = new UniToken(0, token0.publicKey, 8);
    uniToken1 = new UniToken(0, token1.publicKey, 8);
    uniToken2 = new UniToken(0, token2.publicKey, 8);
    console.log("Token 0", token0.publicKey.toString());
    console.log("Token 1", token1.publicKey.toString());
    // console.log("Token 2", token2.publicKey.toString());

    while (token1.publicKey.toString() >= token2.publicKey.toString()) {
      token2 = await Token.createMint(
        connection,
        mintAuthority,
        mintAuthority.publicKey,
        null,
        8,
        TOKEN_PROGRAM_ID
      );
    }
    console.log("Token 2", token2.publicKey.toString());
  });

  it("creates token accounts for position minter and airdrops to them", async () => {
    minterWallet0 = await token0.createAssociatedTokenAccount(owner);
    minterWallet1 = await token1.createAssociatedTokenAccount(owner);
    minterWallet2 = await token2.createAssociatedTokenAccount(owner);
    await token0.mintTo(minterWallet0, mintAuthority, [], 100_000_000);
    await token1.mintTo(minterWallet1, mintAuthority, [], 100_000_000);
    await token2.mintTo(minterWallet2, mintAuthority, [], 100_000_000);
  });

  it("derive pool address", async () => {
    [poolAState, poolAStateBump] = await PublicKey.findProgramAddress(
      [
        POOL_SEED,
        ammConfig.toBuffer(),
        token0.publicKey.toBuffer(),
        token1.publicKey.toBuffer(),
        u32ToSeed(fee),
      ],
      program.programId
    );
    console.log("got poolA address", poolAState.toString());

    [poolBState, poolBStateBump] = await PublicKey.findProgramAddress(
      [
        POOL_SEED,
        ammConfig.toBuffer(),
        token1.publicKey.toBuffer(),
        token2.publicKey.toBuffer(),
        u32ToSeed(fee),
      ],
      program.programId
    );
    console.log("got poolB address", poolBState.toString());
  });

  it("derive vault addresses", async () => {
    [vaultA0, _bumpA0] = await PublicKey.findProgramAddress(
      [POOL_VAULT_SEED, poolAState.toBuffer(), token0.publicKey.toBuffer()],
      program.programId
    );
    console.log("got poolA vaultA0 address", vaultA0.toString());
    [vaultA1, _bumpA1] = await PublicKey.findProgramAddress(
      [POOL_VAULT_SEED, poolAState.toBuffer(), token1.publicKey.toBuffer()],
      program.programId
    );
    console.log("got poolA vaultA1 address", vaultA1.toString());
    [vaultB1, _bumpB1] = await PublicKey.findProgramAddress(
      [POOL_VAULT_SEED, poolBState.toBuffer(), token1.publicKey.toBuffer()],
      program.programId
    );
    console.log("got poolB vaultB1 address", vaultB1.toString());
    [vaultB2, _bumpB2] = await PublicKey.findProgramAddress(
      [POOL_VAULT_SEED, poolBState.toBuffer(), token2.publicKey.toBuffer()],
      program.programId
    );
    console.log("got poolB vaultB2 address", vaultB2.toString());
  });

  it("Create reward token mints and vault", async () => {
    console.log("creating reward token mints");
    const transferSolTx = new web3.Transaction().add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: mintAuthority.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    );
    transferSolTx.add(
      web3.SystemProgram.transfer({
        fromPubkey: owner,
        toPubkey: notOwner.publicKey,
        lamports: web3.LAMPORTS_PER_SOL,
      })
    );
    await anchor.getProvider().send(transferSolTx);

    rewardToken0 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    );
    rewardToken1 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    );
    rewardToken2 = await Token.createMint(
      connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      8,
      TOKEN_PROGRAM_ID
    );

    console.log("rewardToken0", rewardToken0.publicKey.toString());
    console.log("rewardToken1", rewardToken1.publicKey.toString());
    console.log("rewardToken2", rewardToken2.publicKey.toString());

    [rewardVault0, _reward_bump0] = await PublicKey.findProgramAddress(
      [
        POOL_REWARD_VAULT_SEED,
        poolAState.toBuffer(),
        rewardToken0.publicKey.toBuffer(),
      ],
      program.programId
    );
    console.log("got poolA rewardVault0 address", rewardVault0.toString());
    [rewardVault1, _reward_bump1] = await PublicKey.findProgramAddress(
      [
        POOL_REWARD_VAULT_SEED,
        poolAState.toBuffer(),
        rewardToken1.publicKey.toBuffer(),
      ],
      program.programId
    );
    console.log("got poolA rewardVault1 address", rewardVault1.toString());
    [rewardVault2, _reward_bump2] = await PublicKey.findProgramAddress(
      [
        POOL_REWARD_VAULT_SEED,
        poolAState.toBuffer(),
        rewardToken2.publicKey.toBuffer(),
      ],
      program.programId
    );
    console.log("got poolA rewardVault2 address", rewardVault2.toString());
  });

  it("creates reward token accounts for reward founder and airdrops to them", async () => {
    rewardFounderTokenAccount0 =
      await rewardToken0.createAssociatedTokenAccount(rewardFounder.publicKey);
    rewardFounderTokenAccount1 =
      await rewardToken1.createAssociatedTokenAccount(rewardFounder.publicKey);
    rewardFounderTokenAccount2 =
      await rewardToken2.createAssociatedTokenAccount(rewardFounder.publicKey);
    await rewardToken0.mintTo(
      rewardFounderTokenAccount0,
      mintAuthority,
      [],
      100_000_000
    );
    await rewardToken1.mintTo(
      rewardFounderTokenAccount1,
      mintAuthority,
      [],
      100_000_000
    );
    await rewardToken2.mintTo(
      rewardFounderTokenAccount2,
      mintAuthority,
      [],
      100_000_000
    );

    ownerRewardTokenAccount0 = await rewardToken0.createAssociatedTokenAccount(
      owner
    );
    ownerRewardTokenAccount1 = await rewardToken1.createAssociatedTokenAccount(
      owner
    );
    ownerRewardTokenAccount2 = await rewardToken2.createAssociatedTokenAccount(
      owner
    );
    await rewardToken0.mintTo(
      ownerRewardTokenAccount0,
      mintAuthority,
      [],
      100_000_000
    );
    await rewardToken1.mintTo(
      ownerRewardTokenAccount1,
      mintAuthority,
      [],
      100_000_000
    );
    await rewardToken2.mintTo(
      ownerRewardTokenAccount2,
      mintAuthority,
      [],
      100_000_000
    );
  });

  describe("#create_config", () => {
    // Test for event and owner value
    it("initializes config and emits an event", async () => {
      if (await accountExist(connection, ammConfig)) {
        return;
      }
      console.log("to create amm config account");
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "CreateConfigEvent",
      //     (event, slot) => {
      //       assert((event.oldOwner as web3.PublicKey).equals(new PublicKey(0)));
      //       assert((event.newOwner as web3.PublicKey).equals(owner));

      //       resolve([event, slot]);
      //     }
      //   );
      //   console.log("init factory in listener");
      //   program.methods
      //     .createAmmConfig()
      //     .accounts({
      //       owner,
      //       ammConfig,
      //       systemProgram: SystemProgram.programId,
      //     })
      //     .rpc();
      // });
      // await program.removeEventListener(listener);
      // feeRate/ 1e6 = 0.1
      const tx = await program.rpc.createAmmConfig(100000, {
        accounts: {
          owner,
          ammConfig,
          systemProgram: SystemProgram.programId,
        },
      });
      console.log("init config without listener, tx: ", tx);
      const ammConfigData = await program.account.ammConfig.fetch(ammConfig);
      assert.equal(ammConfigData.bump, ammConfigBump);
      assert(ammConfigData.owner.equals(owner));
      assert.equal(ammConfigData.protocolFeeRate, 100000);
    });

    it("Trying to re-initialize config fails", async () => {
      await expect(
        program.rpc.createAmmConfig({
          accounts: {
            owner,
            ammConfig,
            systemProgram: anchor.web3.SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });
  });

  describe("#set_owner", () => {
    const newOwner = new Keypair();

    it("fails if owner does not sign", async () => {
      const tx = program.transaction.setNewOwner({
        accounts: {
          owner,
          newOwner: newOwner.publicKey,
          ammConfig,
        },
      });
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash;

      await expect(connection.sendTransaction(tx, [])).to.be.rejectedWith(
        Error
      );
    });

    it("fails if caller is not owner", async () => {
      const tx = program.transaction.setNewOwner({
        accounts: {
          owner,
          newOwner: newOwner.publicKey,
          ammConfig,
        },
      });
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash;

      await expect(
        connection.sendTransaction(tx, [notOwner])
      ).to.be.rejectedWith(Error);
    });

    it("fails if correct signer but incorrect owner field", async () => {
      await expect(
        program.rpc.setNewOwner({
          accounts: {
            owner: notOwner.publicKey,
            newOwner: newOwner.publicKey,
            ammConfig,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    // Test for event and updated owner value
    it("updates owner and emits an event", async function () {
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "OwnerChanged",
      //     (event, slot) => {
      //       assert((event.oldOwner as web3.PublicKey).equals(owner));
      //       assert(
      //         (event.newOwner as web3.PublicKey).equals(newOwner.publicKey)
      //       );

      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.setOwner({
      //     accounts: {
      //       owner,
      //       newOwner: newOwner.publicKey,
      //       ammConfig,
      //     },
      //   });
      // });
      // await program.removeEventListener(listener);

      await program.rpc.setNewOwner({
        accounts: {
          owner,
          newOwner: newOwner.publicKey,
          ammConfig,
        },
      });

      const ammConfigData = await program.account.ammConfig.fetch(ammConfig);
      assert(ammConfigData.owner.equals(newOwner.publicKey));
    });

    it("reverts to original owner when signed by the new owner", async () => {
      await program.rpc.setNewOwner({
        accounts: {
          owner: newOwner.publicKey,
          newOwner: owner,
          ammConfig,
        },
        signers: [newOwner],
      });
      const factoryStateData = await program.account.ammConfig.fetch(ammConfig);
      assert(factoryStateData.owner.equals(owner));
    });
  });

  describe("#create_fee_account", () => {
    it("fails if PDA seeds do not match", async () => {
      await expect(
        program.rpc.createFeeAccount(fee + 1, tickSpacing, {
          accounts: {
            owner,
            ammConfig,
            feeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if caller is not owner", async () => {
      const tx = program.transaction.createFeeAccount(fee, tickSpacing, {
        accounts: {
          owner: notOwner.publicKey,
          ammConfig,
          feeState,
          systemProgram: SystemProgram.programId,
        },
        signers: [notOwner],
      });
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash;

      await expect(
        connection.sendTransaction(tx, [notOwner])
      ).to.be.rejectedWith(Error);
    });

    it("fails if fee is too great", async () => {
      const highFee = 1_000_000;
      const [highFeeState, highFeeStateBump] =
        await PublicKey.findProgramAddress(
          [FEE_SEED, u32ToSeed(highFee)],
          program.programId
        );

      await expect(
        program.rpc.createFeeAccount(highFee, tickSpacing, {
          accounts: {
            owner,
            ammConfig,
            feeState: highFeeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if tick spacing is too small", async () => {
      await expect(
        program.rpc.createFeeAccount(fee, 0, {
          accounts: {
            owner,
            ammConfig,
            feeState: feeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if tick spacing is too large", async () => {
      await expect(
        program.rpc.createFeeAccount(fee, 16384, {
          accounts: {
            owner,
            ammConfig,
            feeState: feeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("sets the fee amount and emits an event", async () => {
      if (await accountExist(connection, feeState)) {
        return;
      }
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "FeeAmountEnabled",
      //     (event, slot) => {
      //       assert.equal(event.fee, fee);
      //       assert.equal(event.tickSpacing, tickSpacing);

      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.createFeeAccount(fee, tickSpacing, {
      //     accounts: {
      //       owner,
      //       ammConfig,
      //       feeState,
      //       systemProgram: SystemProgram.programId,
      //     },
      //   });
      // });
      // await program.removeEventListener(listener);
      await program.rpc.createFeeAccount(fee, tickSpacing, {
        accounts: {
          owner,
          ammConfig,
          feeState,
          systemProgram: SystemProgram.programId,
        },
      });
      const feeStateData = await program.account.feeState.fetch(feeState);
      console.log("fee state", feeStateData);
      assert.equal(feeStateData.bump, feeStateBump);
      assert.equal(feeStateData.fee, fee);
      assert.equal(feeStateData.tickSpacing, tickSpacing);
    });

    it("fails if already initialized", async () => {
      await expect(
        program.rpc.createFeeAccount(feeStateBump, fee, tickSpacing, {
          accounts: {
            owner,
            ammConfig,
            feeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("cannot change spacing of a fee tier", async () => {
      await expect(
        program.rpc.createFeeAccount(feeStateBump, fee, tickSpacing + 1, {
          accounts: {
            owner,
            ammConfig,
            feeState,
            systemProgram: SystemProgram.programId,
          },
        })
      ).to.be.rejectedWith(Error);
    });
  });

  describe("#create_pool", () => {
    it("derive first observation slot address", async () => {
      [initialObservationStateA, initialObservationBumpA] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(0)],
          program.programId
        );
      [initialObservationStateB, initialObservationBumpB] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolBState.toBuffer(), u16ToSeed(0)],
          program.programId
        );
    });

    it("fails if tokens are passed in reverse", async () => {
      // Unlike Uniswap, we must pass the tokens by address sort order
      await expect(
        program.rpc.createPool(initialPriceX32, {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token1.publicKey,
            tokenMint1: token0.publicKey,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if token0 == token1", async () => {
      // Unlike Uniswap, we must pass the tokens by address sort order
      await expect(
        program.rpc.createPool(initialPriceX32, {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token0.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if fee account is not create", async () => {
      const [uninitializedFeeState, _] = await PublicKey.findProgramAddress(
        [FEE_SEED, u32ToSeed(fee + 1)],
        program.programId
      );

      await expect(
        program.rpc.createPool(initialPriceX32, {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token0.publicKey,
            feeState: uninitializedFeeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if starting price is too low", async () => {
      await expect(
        program.rpc.createPool(new BN(1), {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);

      await expect(
        program.rpc.createPool(MIN_SQRT_RATIO.subn(1), {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if starting price is too high", async () => {
      await expect(
        program.rpc.createPool(MAX_SQRT_RATIO, {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);

      await expect(
        program.rpc.createPool(new BN(2).pow(new BN(64)).subn(1), {
          // u64::MAX
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("creates a new pool and initializes it with a starting price", async () => {
      if (await accountExist(connection, poolAState)) {
        return;
      }
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "PoolCreatedAndInitialized",
      //     (event, slot) => {
      //       assert((event.token0 as web3.PublicKey).equals(token0.publicKey));
      //       assert((event.token1 as web3.PublicKey).equals(token1.publicKey));
      //       assert.equal(event.fee, fee);
      //       assert.equal(event.tickSpacing, tickSpacing);
      //       assert((event.poolState as web3.PublicKey).equals(poolAState));
      //       assert((event.sqrtPriceX32 as BN).eq(initialPriceX32));
      //       assert.equal(event.tick, initialTick);

      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.createPool(initialPriceX32, {
      //     accounts: {
      //       poolCreator: owner,
      //       tokenMint0: token0.publicKey,
      //       tokenMint1: token1.publicKey,
      //       feeState,
      //       poolState: poolAState,
      //       initialObservationState: initialObservationStateA,
      //       serumMarket: serumMarketA,
      //       tokenVault0: vaultA0,
      //       tokenVault1: vaultA1,
      //       tokenProgram: TOKEN_PROGRAM_ID,
      //       systemProgram: SystemProgram.programId,
      //       rent: web3.SYSVAR_RENT_PUBKEY,
      //     },
      //   });
      // });
      // await program.removeEventListener(listener);
      await program.rpc.createPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          ammConfig: ammConfig,
          tokenMint0: token0.publicKey,
          tokenMint1: token1.publicKey,
          feeState,
          poolState: poolAState,
          initialObservationState: initialObservationStateA,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        },
      });
      // pool state variables
      const poolStateData = await program.account.poolState.fetch(poolAState);
      assert.equal(poolStateData.bump, poolAStateBump);
      assert(poolStateData.tokenMint0.equals(token0.publicKey));
      assert(poolStateData.tokenMint1.equals(token1.publicKey));
      assert(poolStateData.tokenVault0.equals(vaultA0));
      assert(poolStateData.tokenVault1.equals(vaultA1));
      assert.equal(poolStateData.feeRate, fee);
      assert.equal(poolStateData.tickSpacing, tickSpacing);
      assert(poolStateData.liquidity.eqn(0));
      assert(poolStateData.sqrtPrice.eq(initialPriceX32));
      assert.equal(poolStateData.tick, initialTick);
      assert.equal(poolStateData.observationIndex, 0);
      assert.equal(poolStateData.observationCardinality, 1);
      assert.equal(poolStateData.observationCardinalityNext, 1);
      assert(poolStateData.feeGrowthGlobal0.eq(new BN(0)));
      assert(poolStateData.feeGrowthGlobal1.eq(new BN(0)));
      assert(poolStateData.protocolFeesToken0.eq(new BN(0)));
      assert(poolStateData.protocolFeesToken1.eq(new BN(0)));

      // first observations slot
      const observationStateData = await program.account.observationState.fetch(
        initialObservationStateA
      );
      assert.equal(observationStateData.bump, initialObservationBumpA);
      assert.equal(observationStateData.index, 0);
      assert(observationStateData.tickCumulative.eqn(0));
      assert(observationStateData.liquidityCumulative.eqn(0));
      assert(observationStateData.initialized);
      // assert.approximately(
      //   observationStateData.blockTimestamp,
      //   Math.floor(Date.now() / 1000),
      //   60
      // );

      console.log("got pool address", poolAState.toString());
    });

    it("fails if already initialized", async () => {
      await expect(
        program.rpc.createPool(initialPriceX32, {
          accounts: {
            poolCreator: owner,
            ammConfig: ammConfig,
            tokenMint0: token0.publicKey,
            tokenMint1: token1.publicKey,
            feeState,
            poolState: poolAState,
            initialObservationState: initialObservationStateA,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          },
        })
      ).to.be.rejectedWith(Error);
    });
  });
  describe("#reset_sqrt_price_1", () => {
    it("fails if not pool owner", async () => {
      await expect(
        program.methods
          .resetSqrtPrice(new BN(10))
          .accounts({
            owner: notOwner.publicKey,
            ammConfig: ammConfig,
            poolState: poolAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
          })
          .remainingAccounts([])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("reset sqrt price with initialPriceX32 add 10", async () => {
      await program.methods
        .resetSqrtPrice(initialPriceX32.add(new BN(10)))
        .accounts({
          owner: owner,
          ammConfig: ammConfig,
          poolState: poolAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
        })
        .remainingAccounts([])
        .rpc();

      let poolStateData = await program.account.poolState.fetch(poolAState);
      assert(poolStateData.sqrtPrice.eq(initialPriceX32.add(new BN(10))));

      await program.methods
        .resetSqrtPrice(initialPriceX32)
        .accounts({
          owner: owner,
          ammConfig: ammConfig,
          poolState: poolAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
        })
        .remainingAccounts([])
        .rpc();

      poolStateData = await program.account.poolState.fetch(poolAState);
      assert(poolStateData.sqrtPrice.eq(initialPriceX32));
    });
  });

  describe("#increase_observation_cardinality_next", () => {
    it("fails if bump does not produce a PDA with observation state seeds", async () => {
      const [observationState, _] = await PublicKey.findProgramAddress(
        [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(1)],
        program.programId
      );

      await expect(
        program.rpc.increaseObservationCardinalityNext(Buffer.from([0]), {
          accounts: {
            payer: owner,
            poolState: poolAState,
            systemProgram: SystemProgram.programId,
          },
          remainingAccounts: [
            {
              pubkey: observationState,
              isSigner: true,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if bump is valid but account does not match expected address for current cardinality_next", async () => {
      const [_, observationStateBump] = await PublicKey.findProgramAddress(
        [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(1)],
        program.programId
      );
      const fakeAccount = new Keypair();

      await expect(
        program.rpc.increaseObservationCardinalityNext(
          Buffer.from([observationStateBump]),
          {
            accounts: {
              payer: owner,
              poolState: poolAState,
              systemProgram: SystemProgram.programId,
            },
            remainingAccounts: [
              {
                pubkey: fakeAccount.publicKey,
                isSigner: true,
                isWritable: true,
              },
            ],
            signers: [fakeAccount],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("fails if a single address is passed with index greater than cardinality_next", async () => {
      const [observationState2, observationState2Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(2)],
          program.programId
        );

      await expect(
        program.rpc.increaseObservationCardinalityNext(
          Buffer.from([observationState2Bump]),
          {
            accounts: {
              payer: owner,
              poolState: poolAState,
              systemProgram: SystemProgram.programId,
            },
            remainingAccounts: [
              {
                pubkey: observationState2,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("increase cardinality by one", async () => {
      const [observationState0, observationState0Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(0)],
          program.programId
        );
      const firstObservtionBefore =
        await program.account.observationState.fetch(observationState0);

      const [observationState1, observationState1Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(1)],
          program.programId
        );

      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "IncreaseObservationCardinalityNext",
      //     (event, slot) => {
      //       assert.equal(event.observationCardinalityNextOld, 1);
      //       assert.equal(event.observationCardinalityNextNew, 2);
      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.increaseObservationCardinalityNext(
      //     Buffer.from([observationState1Bump]),
      //     {
      //       accounts: {
      //         payer: owner,
      //         poolState: poolAState,
      //         systemProgram: SystemProgram.programId,
      //       },
      //       remainingAccounts: [
      //         {
      //           pubkey: observationState1,
      //           isSigner: false,
      //           isWritable: true,
      //         },
      //       ],
      //     }
      //   );
      // });
      // await program.removeEventListener(listener);
      await program.rpc.increaseObservationCardinalityNext(
        Buffer.from([observationState1Bump]),
        {
          accounts: {
            payer: owner,
            poolState: poolAState,
            systemProgram: SystemProgram.programId,
          },
          remainingAccounts: [
            {
              pubkey: observationState1,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );

      const observationState1Data =
        await program.account.observationState.fetch(observationState1);
      console.log("Observation state 1 data", observationState1Data);
      assert.equal(observationState1Data.bump, observationState1Bump);
      assert.equal(observationState1Data.index, 1);
      assert.equal(observationState1Data.blockTimestamp, 1);
      assert(observationState1Data.tickCumulative.eqn(0));
      assert(observationState1Data.liquidityCumulative.eqn(0));
      assert.isFalse(observationState1Data.initialized);

      const poolStateData = await program.account.poolState.fetch(poolAState);
      assert.equal(poolStateData.observationIndex, 0);
      assert.equal(poolStateData.observationCardinality, 1);
      assert.equal(poolStateData.observationCardinalityNext, 2);

      // does not touch the first observation
      const firstObservtionAfter = await program.account.observationState.fetch(
        observationState0
      );
      assert.deepEqual(firstObservtionAfter, firstObservtionBefore);
    });

    it("fails if accounts are not in ascending order of index", async () => {
      const [observationState2, observationState2Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(2)],
          program.programId
        );
      const [observationState3, observationState3Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(3)],
          program.programId
        );

      await expect(
        program.rpc.increaseObservationCardinalityNext(
          Buffer.from([observationState3Bump, observationState2Bump]),
          {
            accounts: {
              payer: owner,
              poolState: poolAState,
              systemProgram: SystemProgram.programId,
            },
            remainingAccounts: [
              {
                pubkey: observationState3,
                isSigner: false,
                isWritable: true,
              },
              {
                pubkey: observationState2,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("fails if a stray account is present between the array of observation accounts", async () => {
      const [observationState2, observationState2Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(2)],
          program.programId
        );
      const [observationState3, observationState3Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(3)],
          program.programId
        );

      await expect(
        program.rpc.increaseObservationCardinalityNext(
          Buffer.from([observationState2Bump, observationState3Bump]),
          {
            accounts: {
              payer: owner,
              poolState: poolAState,
              systemProgram: SystemProgram.programId,
            },
            remainingAccounts: [
              {
                pubkey: observationState2,
                isSigner: false,
                isWritable: true,
              },
              {
                pubkey: new Keypair().publicKey,
                isSigner: false,
                isWritable: true,
              },
              {
                pubkey: observationState3,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("fails if less than current value of cardinality_next", async () => {
      const [observationState1, observationState1Bump] =
        await PublicKey.findProgramAddress(
          [OBSERVATION_SEED, poolAState.toBuffer(), u16ToSeed(1)],
          program.programId
        );

      await expect(
        program.rpc.increaseObservationCardinalityNext(
          Buffer.from([observationState1Bump]),
          {
            accounts: {
              payer: owner,
              poolState: poolAState,
              systemProgram: SystemProgram.programId,
            },
            remainingAccounts: [
              {
                pubkey: observationState1,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });
  });

  describe("#set_protocol_fee_rate", () => {
    it("cannot be changed by addresses that are not owner", async () => {
      await expect(
        program.rpc.setProtocolFeeRate(6, {
          accounts: {
            owner: notOwner.publicKey,
            ammConfig,
          },
          signers: [notOwner],
        })
      ).to.be.rejectedWith(Error);
    });

    it("cannot be changed out of bounds", async () => {
      await expect(
        program.rpc.setProtocolFeeRate(0, {
          accounts: {
            owner,
            ammConfig,
          },
        })
      ).to.be.rejectedWith(Error);

      await expect(
        program.rpc.setProtocolFeeRate(10000000000, {
          accounts: {
            owner,
            ammConfig,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("can be changed by owner", async () => {
      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener("SetFeeProtocolEvent", (event, slot) => {
      //     assert.equal(event.feeProtocolOld, 3)
      //     assert.equal(event.feeProtocol, 6)

      //     resolve([event, slot]);
      //   });

      //   program.rpc.setFeeProtocol(6, {
      //     accounts: {
      //       owner,
      //       ammConfig,
      //     }
      //   })
      // })
      // await program.removeEventListener(listener)
      // feeRate/ 1e6 = 0.3
      await program.rpc.setProtocolFeeRate(300000, {
        accounts: {
          owner,
          ammConfig,
        },
      });

      const factoryStateData = await program.account.ammConfig.fetch(ammConfig);
      assert.equal(factoryStateData.protocolFeeRate, 300000);
    });
  });

  describe("#initialize_reward", () => {
    it("fails if openTime greater than endTime", async () => {
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 0,
            openTime: new BN(2),
            endTime: new BN(1),
            emissionsPerSecondX32: new BN(1),
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });
    it("fails if endTime less than currentTime", async () => {
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 0,
            openTime: new BN(1),
            endTime: new BN(1),
            emissionsPerSecondX32: new BN(1),
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });
    it("fails if reward index overflow", async () => {
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 3,
            openTime: new BN(1),
            endTime: new BN(getUnixTs() + 100),
            emissionsPerSecondX32: new BN(1),
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("fails if rewardPerSecond is zero", async () => {
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 0,
            openTime: new BN(1),
            endTime: new BN(getUnixTs() + 100),
            emissionsPerSecondX32: new BN(0),
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("fails if rewardPerSecond is zero", async () => {
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 0,
            openTime: new BN(1),
            endTime: new BN(getUnixTs() + 100),
            emissionsPerSecondX32: new BN(0),
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("init reward index 0 not owner authority", async () => {
      const curr_timestamp = await getBlockTimestamp(connection);
      await expect(
        program.methods
          .initializeReward({
            rewardIndex: 0,
            openTime: new BN(curr_timestamp),
            endTime: new BN(curr_timestamp + 3),
            emissionsPerSecondX32: new BN(429496729600), // 100
          })
          .accounts({
            ammConfig: ammConfig,
            rewardFunder: rewardFounder.publicKey,
            funderTokenAccount: rewardFounderTokenAccount0,
            poolState: poolAState,
            rewardTokenMint: rewardToken0.publicKey,
            rewardTokenVault: rewardVault0,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
          })
          .signers([rewardFounder])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("init reward index 0 with rewardPerSecond 100, but not init vault", async () => {
      const curr_timestamp = await getBlockTimestamp(connection);
      console.log("reward index 0, open_time: ", curr_timestamp);
      await program.methods
        .initializeReward({
          rewardIndex: 0,
          openTime: new BN(curr_timestamp),
          endTime: new BN(curr_timestamp + 3),
          emissionsPerSecondX32: new BN(429496729600), // 100
        })
        .accounts({
          ammConfig: ammConfig,
          rewardFunder: owner,
          funderTokenAccount: ownerRewardTokenAccount0,
          poolState: poolAState,
          rewardTokenMint: rewardToken0.publicKey,
          rewardTokenVault: rewardVault0,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([])
        .rpc();

      const poolAStateData = await program.account.poolState.fetch(poolAState);

      assert.equal(
        poolAStateData.rewardInfos[0].tokenVault.toString(),
        rewardVault0.toString()
      );
      assert.equal(
        poolAStateData.rewardInfos[0].tokenMint.toString(),
        rewardToken0.publicKey.toString()
      );
      assert.equal(
        poolAStateData.rewardInfos[0].openTime.toNumber(),
        curr_timestamp
      );
      assert.equal(
        poolAStateData.rewardInfos[0].endTime.toNumber(),
        curr_timestamp + 3
      );
      assert.equal(
        poolAStateData.rewardInfos[0].emissionsPerSecondX32.toNumber(),
        429496729600
      );
    });

    it("init reward index 1 with rewardPerSecond 10 and init amount 100", async () => {
      const curr_timestamp = await getBlockTimestamp(connection);
      console.log("reward index 1, open_time: ", curr_timestamp);
      await program.methods
        .initializeReward({
          rewardIndex: 1,
          openTime: new BN(curr_timestamp),
          endTime: new BN(curr_timestamp + 3),
          emissionsPerSecondX32: new BN(42949672960), // 10
        })
        .accounts({
          ammConfig: ammConfig,
          rewardFunder: owner,
          funderTokenAccount: ownerRewardTokenAccount1,
          poolState: poolAState,
          rewardTokenMint: rewardToken1.publicKey,
          rewardTokenVault: rewardVault1,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([])
        .rpc();
      let poolAStateData = await program.account.poolState.fetch(poolAState);

      assert.equal(
        poolAStateData.rewardInfos[1].tokenVault.toString(),
        rewardVault1.toString()
      );
      assert.equal(
        poolAStateData.rewardInfos[1].tokenMint.toString(),
        rewardToken1.publicKey.toString()
      );
    });

    it("init reward index 2 with rewardPerSecond 100 and init amount 50", async () => {
      const curr_timestamp = await getBlockTimestamp(connection);
      await program.methods
        .initializeReward({
          rewardIndex: 2,
          openTime: new BN(curr_timestamp),
          endTime: new BN(curr_timestamp + 3),
          emissionsPerSecondX32: new BN(4294967296), // 1
        })
        .accounts({
          ammConfig: ammConfig,
          rewardFunder: owner,
          funderTokenAccount: ownerRewardTokenAccount2,
          poolState: poolAState,
          rewardTokenMint: rewardToken2.publicKey,
          rewardTokenVault: rewardVault2,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([])
        .rpc();
      let poolAStateData = await program.account.poolState.fetch(poolAState);

      assert.equal(
        poolAStateData.rewardInfos[2].tokenVault.toString(),
        rewardVault2.toString()
      );
      assert.equal(
        poolAStateData.rewardInfos[2].tokenMint.toString(),
        rewardToken2.publicKey.toString()
      );
    });
  });

  describe("#collect_protocol_fee", () => {
    it("creates token accounts for recipient", async () => {
      feeRecipientWallet0 = await token0.createAssociatedTokenAccount(
        protocolFeeRecipient.publicKey
      );
      feeRecipientWallet1 = await token1.createAssociatedTokenAccount(
        protocolFeeRecipient.publicKey
      );
    });

    it("fails if caller is not owner", async () => {
      await expect(
        program.rpc.collectProtocolFee(MaxU64, MaxU64, {
          accounts: {
            owner: notOwner.publicKey,
            ammConfig,
            poolState: poolAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if vault 0 address is not valid", async () => {
      await expect(
        program.rpc.collectProtocolFee(MaxU64, MaxU64, {
          accounts: {
            owner: notOwner.publicKey,
            ammConfig,
            poolState: poolAState,
            tokenVault0: new Keypair().publicKey,
            tokenVault1: vaultA1,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if vault 1 address is not valid", async () => {
      await expect(
        program.rpc.collectProtocolFee(MaxU64, MaxU64, {
          accounts: {
            owner: notOwner.publicKey,
            ammConfig,
            poolState: poolAState,
            tokenVault0: vaultA0,
            tokenVault1: new Keypair().publicKey,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
        })
      ).to.be.rejectedWith(Error);
    });

    it("no token transfers if no fees", async () => {
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "CollectProtocolEvent",
      //     (event, slot) => {
      //       assert((event.poolState as web3.PublicKey).equals(poolAState));
      //       assert((event.sender as web3.PublicKey).equals(owner));
      //       assert((event.amount0 as BN).eqn(0));
      //       assert((event.amount1 as BN).eqn(0));

      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.collectProtocolFee(MaxU64, MaxU64, {
      //     accounts: {
      //       owner,
      //       ammConfig,
      //       poolState: poolAState,
      //       tokenVault0: vaultA0,
      //       tokenVault1: vaultA1,
      //       recipientWallet0: feeRecipientWallet0,
      //       recipientWallet1: feeRecipientWallet1,
      //       tokenProgram: TOKEN_PROGRAM_ID,
      //     },
      //   });
      // });
      // await program.removeEventListener(listener);
      await program.rpc.collectProtocolFee(MaxU64, MaxU64, {
        accounts: {
          owner,
          ammConfig,
          poolState: poolAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
      });
      const poolStateData = await program.account.poolState.fetch(poolAState);
      assert(poolStateData.protocolFeesToken0.eqn(0));
      assert(poolStateData.protocolFeesToken1.eqn(0));

      const recipientWallet0Info = await token0.getAccountInfo(
        feeRecipientWallet0
      );
      const recipientWallet1Info = await token1.getAccountInfo(
        feeRecipientWallet1
      );
      assert(recipientWallet0Info.amount.eqn(0));
      assert(recipientWallet1Info.amount.eqn(0));
    });

    // TODO remaining tests after swap component is ready
  });

  it("find program accounts addresses for position creation", async () => {
    [tickLowerAState, tickLowerAStateBump] = await PublicKey.findProgramAddress(
      [TICK_SEED, poolAState.toBuffer(), u32ToSeed(tickLower)],
      program.programId
    );
    console.log("tickLowerAState key: ", tickLowerAState.toString());
    [tickLowerBState, tickLowerBStateBump] = await PublicKey.findProgramAddress(
      [TICK_SEED, poolBState.toBuffer(), u32ToSeed(tickLower)],
      program.programId
    );
    console.log("tickLowerBState key: ", tickLowerBState.toString());
    [tickUpperAState, tickUpperAStateBump] = await PublicKey.findProgramAddress(
      [TICK_SEED, poolAState.toBuffer(), u32ToSeed(tickUpper)],
      program.programId
    );
    console.log("tickUpperAState key: ", tickUpperAState.toString());
    [tickUpperBState, tickUpperBStateBump] = await PublicKey.findProgramAddress(
      [TICK_SEED, poolBState.toBuffer(), u32ToSeed(tickUpper)],
      program.programId
    );
    console.log("tickUpperBState key: ", tickUpperBState.toString());
    [bitmapLowerAState, bitmapLowerABump] = await PublicKey.findProgramAddress(
      [BITMAP_SEED, poolAState.toBuffer(), u16ToSeed(wordPosLower)],
      program.programId
    );
    console.log("bitmapLowerAState key: ", bitmapLowerAState.toString());
    [bitmapUpperAState, bitmapUpperABump] = await PublicKey.findProgramAddress(
      [BITMAP_SEED, poolAState.toBuffer(), u16ToSeed(wordPosUpper)],
      program.programId
    );
    console.log("bitmapUpperAState key: ", bitmapUpperAState.toString());
    [bitmapLowerBState, bitmapLowerBBump] = await PublicKey.findProgramAddress(
      [BITMAP_SEED, poolBState.toBuffer(), u16ToSeed(wordPosLower)],
      program.programId
    );
    console.log("bitmapLowerBState key: ", bitmapLowerBState.toString());
    [bitmapUpperBState, bitmapUpperBBump] = await PublicKey.findProgramAddress(
      [BITMAP_SEED, poolBState.toBuffer(), u16ToSeed(wordPosUpper)],
      program.programId
    );
    console.log("bitmapUpperBState key: ", bitmapUpperBState.toString());
    [protocolPositionAState, corePositionABump] =
      await PublicKey.findProgramAddress(
        [
          POSITION_SEED,
          poolAState.toBuffer(),
          u32ToSeed(tickLower),
          u32ToSeed(tickUpper),
        ],
        program.programId
      );
    console.log(
      "protocolPositionAState key: ",
      protocolPositionAState.toString(),
      "u32ToSeed(tickLower)",
      u32ToSeed(tickLower),
      "u32ToSeed(tickUpper)",
      u32ToSeed(tickUpper)
    );
    [protocolPositionBState, corePositionBBump] =
      await PublicKey.findProgramAddress(
        [
          POSITION_SEED,
          poolBState.toBuffer(),
          u32ToSeed(tickLower),
          u32ToSeed(tickUpper),
        ],
        program.programId
      );
    console.log(
      "protocolPositionBState key: ",
      protocolPositionBState.toString()
    );
    positionANftAccount = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      nftMintAKeypair.publicKey,
      owner
    );
    positionBNftAccount = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      nftMintBKeypair.publicKey,
      owner
    );

    const nftMint = new Token(
      connection,
      nftMintAKeypair.publicKey,
      TOKEN_PROGRAM_ID,
      mintAuthority
    );

    metadataAccount = (
      await web3.PublicKey.findProgramAddress(
        [
          Buffer.from("metadata"),
          metaplex.programs.metadata.MetadataProgram.PUBKEY.toBuffer(),
          nftMintAKeypair.publicKey.toBuffer(),
        ],
        metaplex.programs.metadata.MetadataProgram.PUBKEY
      )
    )[0];

    metadataBAccount = (
      await web3.PublicKey.findProgramAddress(
        [
          Buffer.from("metadata"),
          metaplex.programs.metadata.MetadataProgram.PUBKEY.toBuffer(),
          nftMintBKeypair.publicKey.toBuffer(),
        ],
        metaplex.programs.metadata.MetadataProgram.PUBKEY
      )
    )[0];

    [personalPositionAState, personalPositionABump] =
      await PublicKey.findProgramAddress(
        [POSITION_SEED, nftMintAKeypair.publicKey.toBuffer()],
        program.programId
      );
    [personalPositionBState, personalPositionBBump] =
      await PublicKey.findProgramAddress(
        [POSITION_SEED, nftMintBKeypair.publicKey.toBuffer()],
        program.programId
      );
  });

  describe("#create_personal_position", () => {
    it("generate observation PDAs", async () => {
      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      lastObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed(observationIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext),
          ],
          program.programId
        )
      )[0];
    });

    it("create personal position", async () => {
      console.log("word upper:", wordPosUpper);
      console.log("minterWallet0: ", minterWallet0.toString());
      console.log("minterWallet0: ", minterWallet1.toString());
      console.log("pool vaultA0: ", vaultA0.toString());
      console.log("pool vaultA1: ", vaultA1.toString());

      // pool currency price: 4297115210, pool currency tick: 10
      // tick_lower: 0,  price: 4294967296
      // tick_upper: 10,  price: 4297115210
      // so only token_1 be added.
      const price_lower = TickMath.getSqrtRatioAtTick(tickLower);
      const price_upper = TickMath.getSqrtRatioAtTick(tickUpper);
      //  ΔL = Δy / (√P_upper - √P_lower)
      // ΔL = 1000000 /(4297115210 - 4294967296) * 4294967296 = 1999599283.770207
      const expectLiquity = maxLiquidityForAmounts(
        JSBI.BigInt(4297115210),
        price_lower,
        price_upper,
        JSBI.BigInt(amount0Desired),
        JSBI.BigInt(amount1Desired),
        true
      );
      console.log(
        "price_lower:",
        JSBI.toNumber(price_lower),
        "price_upper:",
        JSBI.toNumber(price_upper),
        "curr_price:",
        4297115210,
        "expectLiquity",
        JSBI.toNumber(expectLiquity)
      );
      const tx = await program.methods
        .createPosition(
          tickLower,
          tickUpper,
          wordPosLower,
          wordPosUpper,
          amount0Desired,
          amount1Desired,
          amount0Minimum,
          amount1Minimum
        )
        .accounts({
          minter: owner,
          positionNftOwner: owner,
          ammConfig,
          positionNftMint: nftMintAKeypair.publicKey,
          positionNftAccount: positionANftAccount,
          metadataAccount,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          tokenAccount0: minterWallet0,
          tokenAccount1: minterWallet1,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          lastObservationState: lastObservationAState,
          personalPositionState: personalPositionAState,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          metadataProgram: metaplex.programs.metadata.MetadataProgram.PUBKEY,
        })
        .remainingAccounts([
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ])
        .signers([nftMintAKeypair])
        .rpc();
      console.log("create position, tx:", tx);
      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener("IncreaseLiquidityEvent", (event, slot) => {
      //     assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
      //     assert((event.amount0 as BN).eqn(0))
      //     assert((event.amount1 as BN).eq(amount1Desired))
      //     console.log("liquidity: ",event.liquidity, "amount0Desired:",event.amount0, "amount1Desired: ", event.amount1,"nft_mint",event.tokenId.toString())
      //     resolve([event, slot]);
      //   });

      //   program.rpc.createTokenizedPosition(
      //     amount0Desired,
      //     amount1Desired,
      //     amount0Minimum,
      //     amount1Minimum,
      //     {
      //       accounts: {
      //         minter: owner,
      //         recipient: owner,
      //         ammConfig,
      //         nftMint: nftMintAKeypair.publicKey,
      //         nftAccount: positionANftAccount,
      //         poolState: poolAState,
      //         protocolPositionState: corePositionAState,
      //         tickLowerState: tickLowerAState,
      //         tickUpperState: tickUpperAState,
      //         bitmapLowerState: bitmapLowerAState,
      //         bitmapUpperState: bitmapUpperAState,
      //         tokenAccount0: minterWallet0,
      //         tokenAccount1: minterWallet1,
      //         tokenVault0: vaultA0,
      //         tokenVault1: vaultA1,
      //         lastObservationState: lastObservationAState,
      //         personalPositionState: tokenizedPositionAState,
      //         systemProgram: SystemProgram.programId,
      //         rent: web3.SYSVAR_RENT_PUBKEY,
      //         tokenProgram: TOKEN_PROGRAM_ID,
      //         associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      //       },
      //       remainingAccounts: [
      //         {
      //           pubkey: nextObservationAState,
      //           isSigner: false,
      //           isWritable: true,
      //         },
      //       ],
      //       signers: [nftMintAKeypair],
      //     }
      //   );
      // })
      // await program.removeEventListener(listener)
      let poolAStateData = await program.account.poolState.fetch(poolAState);
      assert.equal(poolAStateData.liquidity.toNumber(), 0);
      const nftMint = new Token(
        connection,
        nftMintAKeypair.publicKey,
        TOKEN_PROGRAM_ID,
        new Keypair()
      );
      const nftMintInfo = await nftMint.getMintInfo();
      assert.equal(nftMintInfo.decimals, 0);
      const nftAccountInfo = await nftMint.getAccountInfo(positionANftAccount);
      console.log("NFT account info", nftAccountInfo);
      assert(nftAccountInfo.amount.eqn(1));

      const personalPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );
      console.log("Tokenized position", personalPositionData);
      console.log(
        "liquidity inside position: ",
        personalPositionData.liquidity.toNumber(),
        " expect:",
        expectLiquity
      );
      assert.equal(personalPositionData.liquidity.toNumber(), 1999599283);
      assert.equal(personalPositionData.bump, personalPositionABump);
      assert(personalPositionData.poolId.equals(poolAState));
      assert(personalPositionData.mint.equals(nftMintAKeypair.publicKey));
      assert.equal(personalPositionData.tickLower, tickLower);
      assert.equal(personalPositionData.tickUpper, tickUpper);
      assert(personalPositionData.feeGrowthInside0Last.eqn(0));
      assert(personalPositionData.feeGrowthInside1Last.eqn(0));
      assert(personalPositionData.tokenFeesOwed0.eqn(0));
      assert(personalPositionData.tokenFeesOwed1.eqn(0));

      const vault0State = await token0.getAccountInfo(vaultA0);
      assert(vault0State.amount.eqn(0));
      const vault1State = await token1.getAccountInfo(vaultA1);
      assert(vault1State.amount.eqn(1_000_000));

      const tickLowerData = await program.account.tickState.fetch(
        tickLowerAState
      );
      console.log("Tick lower", tickLowerData);
      assert.equal(
        tickLowerData.liquidityNet.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      assert.equal(
        tickLowerData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      const tickUpperData = await program.account.tickState.fetch(
        tickUpperAState
      );
      console.log("Tick upper", tickUpperData);
      assert.equal(
        tickUpperData.liquidityNet.toNumber(),
        personalPositionData.liquidity.neg().toNumber()
      );
      assert.equal(
        tickUpperData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );

      // check if ticks are correctly initialized on the bitmap
      const tickLowerBitmapData = await program.account.tickBitmapState.fetch(
        bitmapLowerAState
      );
      const bitPosLower = (tickLower / tickSpacing) % 256;
      const bitPosUpper = (tickUpper / tickSpacing) % 256;
      console.log("tickLowerBitmapData:", tickLowerBitmapData);
      console.log("bitPosLower:", bitPosLower);
      console.log("bitPosUpper:", bitPosUpper);
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

      // const corePositionData = await program.account.positionState.fetch(corePositionAState)
      // console.log('Core position data', corePositionData)

      // TODO test remaining fields later
      // Look at uniswap tests for reference
    });
  });

  const nftMint = new Token(
    connection,
    nftMintAKeypair.publicKey,
    TOKEN_PROGRAM_ID,
    notOwner
  );
  
  describe("#increase_liquidity", () => {
    it("update observation accounts", async () => {
      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      const { blockTimestamp: lastBlockTime } =
        await program.account.observationState.fetch(lastObservationAState);

      const slot = await connection.getSlot();
      const blockTimestamp = await connection.getBlockTime(slot);

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (
        Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) &&
        lastBlockTime % 14 >= 11
      ) {
        await new Promise((r) => setTimeout(r, 3000));
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),
              u16ToSeed(observationIndex),
            ],
            program.programId
          )
        )[0];

        nextObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),
              u16ToSeed((observationIndex + 1) % observationCardinalityNext),
            ],
            program.programId
          )
        )[0];
      }
    });

    it("Add token 1 to the position", async () => {
      const tx = await program.rpc.increaseLiquidity(
        amount0Desired,
        amount1Desired,
        amount0Minimum,
        amount1Minimum,
        {
          accounts: {
            payer: owner,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            tokenAccount0: minterWallet0,
            tokenAccount1: minterWallet1,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            lastObservationState: lastObservationAState,
            personalPositionState: personalPositionAState,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );
      console.log("increaseLiquidity tx: ", tx);

      const personalPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );
      console.log("Tokenized position", personalPositionData);
      console.log(
        "liquidity inside position",
        personalPositionData.liquidity.toNumber()
      );
      assert.equal(personalPositionData.liquidity.toNumber(), 3999198566);

      const tickLowerData = await program.account.tickState.fetch(
        tickLowerAState
      );
      console.log("Tick lower", tickLowerData);
      assert.equal(
        tickLowerData.liquidityNet.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      assert.equal(
        tickLowerData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      const tickUpperData = await program.account.tickState.fetch(
        tickUpperAState
      );
      console.log("Tick upper", tickUpperData);
      assert.equal(
        tickUpperData.liquidityNet.toNumber(),
        personalPositionData.liquidity.neg().toNumber()
      );
      assert.equal(
        tickUpperData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      // let listener: number
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener("IncreaseLiquidityEvent", (event, slot) => {
      //     assert((event.tokenId as web3.PublicKey).equals(nftMintAKeypair.publicKey))
      //     assert((event.amount0 as BN).eqn(0))
      //     assert((event.amount1 as BN).eq(amount1Desired))

      //     resolve([event, slot]);
      //   });

      //   program.rpc.increaseLiquidity(
      //     amount0Desired,
      //     amount1Desired,
      //     amount0Minimum,
      //     amount1Minimum,
      //     deadline, {
      //     accounts: {
      //       payer: owner,
      //       ammConfig,
      //       poolState: poolAState,
      //       protocolPositionState: corePositionAState,
      //       tickLowerState: tickLowerAState,
      //       tickUpperState: tickUpperAState,
      //       bitmapLowerState: bitmapLowerAState,
      //       bitmapUpperState: bitmapUpperAState,
      //       tokenAccount0: minterWallet0,
      //       tokenAccount1: minterWallet1,
      //       tokenVault0: vaultA0,
      //       tokenVault1: vaultA1,
      //       lastObservationState: latestObservationAState,
      //       nextObservationState: nextObservationAState,
      //       personalPositionState: tokenizedPositionAState,
      //       program: program.programId,
      //       tokenProgram: TOKEN_PROGRAM_ID,
      //     },
      //   }
      //   )
      // })
      // await program.removeEventListener(listener)

      // const vault0State = await token0.getAccountInfo(vaultA0)
      // assert(vault0State.amount.eqn(0))
      // const vault1State = await token1.getAccountInfo(vaultA1)
      // assert(vault1State.amount.eqn(2_000_000))

      // TODO test remaining fields later
      // Look at uniswap tests for reference
    });

    // To check slippage, we must add liquidity in a price range around
    // current price
  });
  describe("#reset_sqrt_price_2", () => {
    it("fails if pool has liquitity", async () => {
      await expect(
        program.methods
          .resetSqrtPrice(new BN(10))
          .accounts({
            owner: owner,
            ammConfig: ammConfig,
            poolState: poolAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
          })
          .remainingAccounts([])
          .rpc()
      ).to.be.rejectedWith(Error);
    });
  });
  describe("#decrease_liquidity", () => {
    const liquidity = new BN(1999599283);
    const amount1Desired = new BN(999999);

    it("update observation accounts", async () => {
      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      const { blockTimestamp: lastBlockTime } =
        await program.account.observationState.fetch(lastObservationAState);

      const slot = await connection.getSlot();
      const blockTimestamp = await connection.getBlockTime(slot);

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (
        Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) &&
        lastBlockTime % 14 >= 11
      ) {
        await new Promise((r) => setTimeout(r, 3000));
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),
              u16ToSeed(observationIndex),
            ],
            program.programId
          )
        )[0];

        nextObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),
              u16ToSeed((observationIndex + 1) % observationCardinalityNext),
            ],
            program.programId
          )
        )[0];
      }
    });

    it("fails if not called by the owner", async () => {
      await expect(
        program.rpc.decreaseLiquidity(liquidity, new BN(0), amount1Desired, {
          accounts: {
            ownerOrDelegate: notOwner.publicKey,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: minterWallet0,
            recipientTokenAccount1: minterWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if past slippage tolerance", async () => {
      await expect(
        program.rpc.decreaseLiquidity(
          liquidity,
          new BN(0),
          new BN(1_000_000), // 999_999 available
          {
            accounts: {
              ownerOrDelegate: owner,
              nftAccount: positionANftAccount,
              personalPositionState: personalPositionAState,
              ammConfig,
              poolState: poolAState,
              protocolPositionState: protocolPositionAState,
              tickLowerState: tickLowerAState,
              tickUpperState: tickUpperAState,
              bitmapLowerState: bitmapLowerAState,
              bitmapUpperState: bitmapUpperAState,
              lastObservationState: lastObservationAState,
              tokenVault0: vaultA0,
              tokenVault1: vaultA1,
              recipientTokenAccount0: minterWallet0,
              recipientTokenAccount1: minterWallet1,
              tokenProgram: TOKEN_PROGRAM_ID,
            },
            remainingAccounts: [
              {
                pubkey: nextObservationAState,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("generate a temporary NFT account for testing", async () => {
      temporaryNftHolder = await nftMint.createAssociatedTokenAccount(
        mintAuthority.publicKey
      );
    });

    it("fails if NFT token account for the user is empty", async () => {
      const transferTx = new web3.Transaction();
      transferTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      transferTx.add(
        Token.createTransferInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          temporaryNftHolder,
          owner,
          [],
          1
        )
      );

      await anchor.getProvider().send(transferTx);

      await expect(
        program.rpc.decreaseLiquidity(liquidity, new BN(0), amount1Desired, {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount, // no balance
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: minterWallet0,
            recipientTokenAccount1: minterWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);

      // send the NFT back to the original owner
      await nftMint.transfer(
        temporaryNftHolder,
        positionANftAccount,
        mintAuthority,
        [],
        1
      );
    });

    it("burn half of the position liquidity as owner", async () => {
      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "DecreaseLiquidityEvent",
      //     (event, slot) => {
      //       assert(
      //         (event.tokenId as web3.PublicKey).equals(
      //           nftMintAKeypair.publicKey
      //         )
      //       );
      //       assert((event.liquidity as BN).eq(liquidity));
      //       assert((event.amount0 as BN).eqn(0));
      //       assert((event.amount1 as BN).eq(amount1Desired));

      //       resolve([event, slot]);
      //     }
      //   );

      //   program.rpc.decreaseLiquidity(
      //     liquidity,
      //     new BN(0),
      //     amount1Desired,
      //     {
      //       accounts: {
      //         ownerOrDelegate: owner,
      //         nftAccount: positionANftAccount,
      //         personalPositionState: tokenizedPositionAState,
      //         ammConfig,
      //         poolState: poolAState,
      //         protocolPositionState: corePositionAState,
      //         tickLowerState: tickLowerAState,
      //         tickUpperState: tickUpperAState,
      //         bitmapLowerState: bitmapLowerAState,
      //         bitmapUpperState: bitmapUpperAState,
      //         lastObservationState: lastObservationAState,
      //         program: program.programId,
      //       },
      //       remainingAccounts: [
      //         {
      //           pubkey: nextObservationAState,
      //           isSigner: false,
      //           isWritable: true,
      //         },
      //       ],
      //     }
      //   );
      // });
      // await program.removeEventListener(listener);

      const recipientWallet0BalanceBefer = await token0.getAccountInfo(
        minterWallet0
      );
      const recipientWallet1BalanceBefer = await token1.getAccountInfo(
        minterWallet1
      );
      await program.rpc.decreaseLiquidity(
        liquidity,
        new BN(0),
        amount1Desired,
        {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: minterWallet0,
            recipientTokenAccount1: minterWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );

      const personalPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );

      assert.equal(personalPositionData.liquidity.toNumber(), 1999599283);
      const recipientWallet0BalanceAfter = await token0.getAccountInfo(
        minterWallet0
      );
      const recipientWallet1BalanceAfter = await token1.getAccountInfo(
        minterWallet1
      );
      assert(personalPositionData.tokenFeesOwed0.eqn(0));
      assert.equal(personalPositionData.tokenFeesOwed1.toNumber(), 0);

      assert.equal(
        recipientWallet0BalanceAfter.amount.toNumber(),
        recipientWallet0BalanceBefer.amount.toNumber()
      );
      assert.equal(
        recipientWallet1BalanceAfter.amount.toNumber(),
        recipientWallet1BalanceBefer.amount.toNumber() + 999999
      );

      const proctocolPositionData =
        await program.account.procotolPositionState.fetch(
          protocolPositionAState
        );
      assert.equal(proctocolPositionData.liquidity.toNumber(), 1999599283);
      assert.equal(proctocolPositionData.tokenFeesOwed0.toNumber(), 0);
      assert.equal(proctocolPositionData.tokenFeesOwed1.toNumber(), 0);

      const tickLowerData = await program.account.tickState.fetch(
        tickLowerAState
      );
      console.log("Tick lower", tickLowerData);
      assert.equal(tickLowerData.liquidityNet.toNumber(), 1999599283);
      assert.equal(
        tickLowerData.liquidityNet.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      assert.equal(
        tickLowerData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
      const tickUpperData = await program.account.tickState.fetch(
        tickUpperAState
      );
      console.log("Tick upper", tickUpperData);
      assert.equal(tickUpperData.liquidityNet.toNumber(), -1999599283);
      assert.equal(
        tickUpperData.liquidityNet.toNumber(),
        personalPositionData.liquidity.neg().toNumber()
      );
      assert.equal(
        tickUpperData.liquidityGross.toNumber(),
        personalPositionData.liquidity.toNumber()
      );
    });

    it("fails if 0 tokens are delegated", async () => {
      const approveTx = new web3.Transaction();
      approveTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      approveTx.add(
        Token.createApproveInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          mintAuthority.publicKey,
          owner,
          [],
          0
        )
      );
      await anchor.getProvider().send(approveTx);

      const tx = program.transaction.decreaseLiquidity(
        new BN(1_000),
        new BN(0),
        new BN(0),
        {
          accounts: {
            ownerOrDelegate: mintAuthority.publicKey,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: minterWallet0,
            recipientTokenAccount1: minterWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );
      await expect(
        connection.sendTransaction(tx, [mintAuthority])
      ).to.be.rejectedWith(Error);
      // TODO see why errors inside functions are not propagating outside
    });

    it("burn liquidity as the delegated authority", async () => {
      const approveTx = new web3.Transaction();
      approveTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      approveTx.add(
        Token.createApproveInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          mintAuthority.publicKey,
          owner,
          [],
          1
        )
      );
      await anchor.getProvider().send(approveTx);

      const recipientWallet0BalanceBefore = await token0.getAccountInfo(
        minterWallet0
      );
      const recipientWallet1BalanceBefore = await token1.getAccountInfo(
        minterWallet1
      );
      const vault1BalanceBefore = await token1.getAccountInfo(vaultA1);
      // will decrease amount 1 with 500
      // liquidity * (pirce_upper - price_low) / Q32
      // 1_000_000 * (4297115210 - 4294967296) / 4294967296  = 500.10019913315773
      const tx = await program.methods
        .decreaseLiquidity(new BN(1_000_000), new BN(0), new BN(0))
        .accounts({
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          personalPositionState: personalPositionAState,
          ammConfig,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: minterWallet0,
          recipientTokenAccount1: minterWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ])
        .signers([mintAuthority])
        .rpc();

      const personalPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );

      const recipientWallet0BalanceAfter = await token0.getAccountInfo(
        minterWallet0
      );
      const recipientWallet1BalanceAfter = await token1.getAccountInfo(
        minterWallet1
      );
      const vault1BalanceAfter = await token1.getAccountInfo(vaultA1);

      assert(personalPositionData.tokenFeesOwed0.eqn(0));
      assert.equal(personalPositionData.tokenFeesOwed1.toNumber(), 0);
      assert.equal(personalPositionData.liquidity.toNumber(), 1998599283);

      assert.equal(
        recipientWallet0BalanceAfter.amount.toNumber(),
        recipientWallet0BalanceBefore.amount.toNumber()
      );
      assert.equal(
        vault1BalanceAfter.amount.toNumber(),
        vault1BalanceBefore.amount.toNumber() - 500
      );
      assert.equal(
        recipientWallet1BalanceAfter.amount.toNumber(),
        recipientWallet1BalanceBefore.amount.toNumber() + 500
      );
      
      const proctocolPositionData =
        await program.account.procotolPositionState.fetch(
          protocolPositionAState
        );

      assert.equal(proctocolPositionData.tokenFeesOwed0.toNumber(), 0);
      assert.equal(proctocolPositionData.tokenFeesOwed1.toNumber(), 0);

      // let listener: number;
      // let [_event, _slot] = await new Promise((resolve, _reject) => {
      //   listener = program.addEventListener(
      //     "DecreaseLiquidityEvent",
      //     (event, slot) => {
      //       resolve([event, slot]);
      //     }
      //   );

      //   const tx = program.transaction.decreaseLiquidity(
      //     new BN(1_000_000),
      //     new BN(0),
      //     new BN(0),
      //     {
      //       accounts: {
      //         ownerOrDelegate: mintAuthority.publicKey,
      //         nftAccount: positionANftAccount,
      //         personalPositionState: tokenizedPositionAState,
      //         ammConfig,
      //         poolState: poolAState,
      //         protocolPositionState: corePositionAState,
      //         tickLowerState: tickLowerAState,
      //         tickUpperState: tickUpperAState,
      //         bitmapLowerState: bitmapLowerAState,
      //         bitmapUpperState: bitmapUpperAState,
      //         lastObservationState: lastObservationAState,
      //         program: program.programId,
      //       },
      //       remainingAccounts: [
      //         {
      //           pubkey: nextObservationAState,
      //           isSigner: false,
      //           isWritable: true,
      //         },
      //       ],
      //     }
      //   );
      //   connection.sendTransaction(tx, [mintAuthority]);
      // });
      // await program.removeEventListener(listener);
    });

    it("fails if delegation is revoked", async () => {
      const revokeTx = new web3.Transaction();
      revokeTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      revokeTx.add(
        Token.createRevokeInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          owner,
          []
        )
      );
      await anchor.getProvider().send(revokeTx);

      const tx = program.transaction.decreaseLiquidity(
        new BN(1_000_000),
        new BN(0),
        new BN(0),
        {
          accounts: {
            ownerOrDelegate: mintAuthority.publicKey,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: minterWallet0,
            recipientTokenAccount1: minterWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );
      // TODO check for 'Not approved' error
      await expect(
        connection.sendTransaction(tx, [mintAuthority])
      ).to.be.rejectedWith(Error);
    });
  });
 
  describe("#swap_base_input_single", () => {
    // before swapping, current tick = 10 and price = 4297115210
    // active ticks are 0 and 10
    // entire liquidity is in token_1

    it("fails if limit price is greater than current pool price", async () => {
      const amountIn = new BN(100_000);
      const amountOutMinimum = new BN(0);
      const sqrtPriceLimitX32 = new BN(4297115220);

      await expect(
        program.rpc.swap(
          // true,
          amountIn,
          amountOutMinimum,
          sqrtPriceLimitX32,
          true,
          {
            accounts: {
              signer: owner,
              ammConfig,
              poolState: poolAState,
              inputTokenAccount: minterWallet0,
              outputTokenAccount: minterWallet1,
              inputVault: vaultA0,
              outputVault: vaultA1,
              lastObservationState: lastObservationAState,
              tokenProgram: TOKEN_PROGRAM_ID,
            },
            remainingAccounts: [
              {
                pubkey: nextObservationAState,
                isSigner: false,
                isWritable: true,
              },
              {
                pubkey: bitmapLowerAState,
                isSigner: false,
                isWritable: true,
              },
              // price moves downwards in zero for one swap
              {
                pubkey: tickUpperAState,
                isSigner: false,
                isWritable: true,
              },
              {
                pubkey: tickLowerAState,
                isSigner: false,
                isWritable: true,
              },
            ],
          }
        )
      ).to.be.rejectedWith(Error);
    });

    it("swap upto a limit price for a zero to one swap", async () => {
      const amountIn = new BN(100_000);
      const amountOutMinimum = new BN(0);
      const sqrtPriceLimitX32 = new BN(4297115200); // current price is 4297115210

      const tickDataProvider = new SolanaTickDataProvider(program, {
        key: poolAState,
        token0: token0.publicKey,
        token1: token1.publicKey,
        fee,
      });

      const {
        tick: currentTick,
        sqrtPrice: currentSqrtPriceX32,
        liquidity: currentLiquidity,
      } = await program.account.poolState.fetch(poolAState);
      await tickDataProvider.eagerLoadCache(currentTick, tickSpacing);
      // output is one tick behind actual (8 instead of 9)
      uniPoolA = new Pool(
        uniToken0,
        uniToken1,
        fee,
        JSBI.BigInt(currentSqrtPriceX32),
        JSBI.BigInt(currentLiquidity),
        currentTick,
        tickDataProvider
      );

      const [expectedAmountOut, expectedNewPool, bitmapAndTickAccounts] =
        await uniPoolA.getOutputAmount(
          CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber()),
          JSBI.BigInt(sqrtPriceLimitX32)
        );
      assert.equal(
        expectedNewPool.sqrtRatioX32.toString(),
        sqrtPriceLimitX32.toString()
      );
      
      const wallet0BalanceBefore = await token0.getAccountInfo(
        minterWallet0
      );
      const wallet1BalanceBefore = await token1.getAccountInfo(
        minterWallet1
      );
      const vault0BalanceBefore = await token0.getAccountInfo(vaultA0);
      const vault1BalanceBefore = await token1.getAccountInfo(vaultA1);
     
      // limit_price: 297115200, current_price: 4297115210
      // delta x = L / √p - L / √p_upper = L * (√p_upper - √p) / (√p * √p_upper)
      // delta x  = 1998599283 *（4297115210 - 297115200） / (297115200 * 4297115210) = 4.648700779658251 -> ceil = 5
      // delta y = L (√P_upper - √P_lower)
      // delta x  = 1998599283 *（4297115210 - 297115200） /4294967296(Q32) = 4.653351574670523 -> floor = 4
      // fee: 1
      const tx = await program.rpc.swap(
        amountIn,
        amountOutMinimum,
        sqrtPriceLimitX32,
        true,
        {
          accounts: {
            signer: owner,
            ammConfig,
            poolState: poolAState,
            inputTokenAccount: minterWallet0,
            outputTokenAccount: minterWallet1,
            inputVault: vaultA0,
            outputVault: vaultA1,
            lastObservationState: lastObservationAState,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            ...bitmapAndTickAccounts,
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );
      console.log("exactInputSingle, tx: ", tx);
      let poolStateData = await program.account.poolState.fetch(poolAState);
      console.log(
        "poolStateData.tick:",
        poolStateData.tick,
        "poolStateData.sqrtPriceX32: ",
        poolStateData.sqrtPrice.toNumber(),
        "sqrtPriceLimitX32:",
        sqrtPriceLimitX32.toNumber(),
        "expectedAmountOut:",
        JSBI.toNumber(expectedAmountOut.numerator)
      );
      assert.equal(poolStateData.feeGrowthGlobal0.toNumber(), 2);
      assert.equal( JSBI.toNumber(expectedAmountOut.numerator),4)
  
      assert(poolStateData.sqrtPrice.eq(sqrtPriceLimitX32));
      assert.equal(poolStateData.tick, expectedNewPool.tickCurrent);
    
      
      const wallet0BalanceAfter = await token0.getAccountInfo(
        minterWallet0
      );
      const wallet1BalanceAfter = await token1.getAccountInfo(
        minterWallet1
      );
      const vault0BalanceAfter = await token0.getAccountInfo(vaultA0);
      const vault1BalanceAfter = await token1.getAccountInfo(vaultA1);
      assert.equal(wallet0BalanceAfter.amount.toNumber(), wallet0BalanceBefore.amount.toNumber() - 6);
      assert.equal(wallet1BalanceAfter.amount.toNumber(), wallet1BalanceBefore.amount.toNumber() + 4);
      assert.equal(vault0BalanceAfter.amount.toNumber(), vault0BalanceBefore.amount.toNumber() + 6);
      assert.equal(vault1BalanceAfter.amount.toNumber(), vault1BalanceBefore.amount.toNumber() - 4);

      console.log(
        "tick after swap",
        poolStateData.tick,
        "price",
        poolStateData.sqrtPrice.toString()
      );
      uniPoolA = expectedNewPool;
      let poolAStateData = await program.account.poolState.fetch(poolAState);
      assert.equal(poolAStateData.liquidity.toNumber(), 1998599283);

      // console.log("---------------poolAStateData1: ", poolAStateData);
    });
   
    it("performs a zero for one swap without a limit price", async () => {
      let poolStateDataBefore = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price", poolStateDataBefore.sqrtPrice.toNumber());
      console.log("pool tick", poolStateDataBefore.tick);

      const feeGrowthGlobalToken0Before =
        poolStateDataBefore.protocolFeesToken0;
      const vaultBalanceA1Befer = await token1.getAccountInfo(vaultA1);
      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      lastObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed(observationIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext),
          ],
          program.programId
        )
      )[0];

      const amountIn = new BN(100_000);
      const amountOutMinimum = new BN(0);
      const sqrtPriceLimitX32 = new BN(0);

      console.log(
        "pool tick",
        uniPoolA.tickCurrent,
        "price",
        uniPoolA.sqrtRatioX32.toString()
      );
      const [expectedAmountOut, expectedNewPool, bitmapAndTickAccounts] =
        await uniPoolA.getOutputAmount(
          CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber())
          // JSBI.BigInt(sqrtPriceLimitX32)
        );
      console.log("expected pool", expectedNewPool);

      await program.methods
        .swap(amountIn, amountOutMinimum, sqrtPriceLimitX32, true)
        .accounts({
          signer: owner,
          ammConfig,
          poolState: poolAState,
          inputTokenAccount: minterWallet0,
          outputTokenAccount: minterWallet1,
          inputVault: vaultA0,
          outputVault: vaultA1,
          lastObservationState: lastObservationAState,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          ...bitmapAndTickAccounts,
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ])
        .rpc();

      const poolStateDataAfter = await program.account.poolState.fetch(
        poolAState
      );
      console.log("---------------poolAStateData2: ", poolStateDataAfter);
      console.log("pool price after", poolStateDataAfter.sqrtPrice.toNumber());
      console.log("pool tick after", poolStateDataAfter.tick);

      assert.equal(
        poolStateDataAfter.sqrtPrice.toNumber(),
        JSBI.toNumber(expectedNewPool.sqrtRatioX32)
      );
      assert.equal(poolStateDataAfter.tick, expectedNewPool.tickCurrent);

      const feeGrowthGlobalToken0After = poolStateDataAfter.protocolFeesToken0;
      const vaultBalanceA1After = await token1.getAccountInfo(vaultA1);

      assert.equal(
        feeGrowthGlobalToken0After.toNumber(),
        feeGrowthGlobalToken0Before.toNumber() + 15
      );
      assert.equal(
        vaultBalanceA1Befer.amount.toNumber() -
          vaultBalanceA1After.amount.toNumber(),
        JSBI.toNumber(expectedAmountOut.numerator)
      );
      console.log(
        "expectedAmountOut: ",
        JSBI.toNumber(expectedAmountOut.numerator)
      );

      uniPoolA = expectedNewPool;
    });

    it("collect protocol fees after swap", async () => {
      await program.rpc.collectProtocolFee(MaxU64, MaxU64, {
        accounts: {
          owner,
          ammConfig,
          poolState: poolAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
      });
      const poolStateData = await program.account.poolState.fetch(poolAState);
      assert(poolStateData.protocolFeesToken0.eqn(0));
      assert(poolStateData.protocolFeesToken1.eqn(0));

      const recipientWallet0Info = await token0.getAccountInfo(
        feeRecipientWallet0
      );
      const recipientWallet1Info = await token1.getAccountInfo(
        feeRecipientWallet1
      );
      assert.equal(recipientWallet0Info.amount.toNumber(), 15);
      assert(recipientWallet1Info.amount.eqn(0));
    });

  });
  
  describe("#collect_fee", () => {
    it("fails if both amounts are set as 0", async () => {
      await expect(
        program.rpc.collectFee(new BN(0), new BN(0), {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);
    });

    it("fails if signer is not the owner or a delegated authority", async () => {
      const tx = program.transaction.collectFee(new BN(0), new BN(10), {
        accounts: {
          ownerOrDelegate: notOwner.publicKey,
          nftAccount: positionANftAccount,
          personalPositionState: personalPositionAState,
          ammConfig,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ],
      });
      await expect(
        connection.sendTransaction(tx, [notOwner])
      ).to.be.rejectedWith(Error);
    });

    it("fails delegated amount is 0", async () => {
      const approveTx = new web3.Transaction();
      approveTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      approveTx.add(
        Token.createApproveInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          mintAuthority.publicKey,
          owner,
          [],
          0
        )
      );
      await anchor.getProvider().send(approveTx);

      const tx = program.transaction.collectFee(new BN(0), new BN(10), {
        accounts: {
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          personalPositionState: personalPositionAState,
          ammConfig,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ],
      });
      await expect(
        connection.sendTransaction(tx, [mintAuthority])
      ).to.be.rejectedWith(Error);
    });

    it("fails if NFT token account is empty", async () => {
      const transferTx = new web3.Transaction();
      transferTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      transferTx.add(
        Token.createTransferInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          temporaryNftHolder,
          owner,
          [],
          1
        )
      );
      await anchor.getProvider().send(transferTx);

      await expect(
        program.rpc.collectFee(new BN(0), new BN(10), {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);

      // send the NFT back to the original owner
      await nftMint.transfer(
        temporaryNftHolder,
        positionANftAccount,
        mintAuthority,
        [],
        1
      );
    });

    it("collect a portion of owed tokens as owner", async () => {
      const amount0Max = new BN(0);
      const amount1Max = new BN(10);

      await program.rpc.collectFee(amount0Max, amount1Max, {
        accounts: {
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          personalPositionState: personalPositionAState,
          ammConfig,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ],
      });

      const protocolPositionData =
        await program.account.procotolPositionState.fetch(
          protocolPositionAState
        );
      assert.equal(protocolPositionData.tokenFeesOwed0.toNumber(), 35);
      assert.equal(protocolPositionData.tokenFeesOwed1.toNumber(), 0); // minus 10

      const tokenizedPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );
      assert.equal(tokenizedPositionData.tokenFeesOwed0.toNumber(), 35);
      assert.equal(tokenizedPositionData.tokenFeesOwed1.toNumber(), 0);

      const recipientWallet0Info = await token0.getAccountInfo(
        feeRecipientWallet0
      );
      const recipientWallet1Info = await token1.getAccountInfo(
        feeRecipientWallet1
      );
      assert.equal(recipientWallet0Info.amount.toNumber(),15);
      assert.equal(recipientWallet1Info.amount.toNumber(), 0);

      const vault0Info = await token0.getAccountInfo(vaultA0);
      const vault1Info = await token1.getAccountInfo(vaultA1);
      assert.equal(vault0Info.amount.toNumber(), 99991);
      assert.equal(vault1Info.amount.toNumber(), 899453); // minus 10
    });

    it("collect a portion of owed tokens as the delegated authority", async () => {
      const approveTx = new web3.Transaction();
      approveTx.recentBlockhash = (
        await connection.getRecentBlockhash()
      ).blockhash;
      approveTx.add(
        Token.createApproveInstruction(
          TOKEN_PROGRAM_ID,
          positionANftAccount,
          mintAuthority.publicKey,
          owner,
          [],
          1
        )
      );
      await anchor.getProvider().send(approveTx);

      const amount0Max = new BN(0);
      const amount1Max = new BN(10);

      const tx = await program.rpc.collectFee(amount0Max, amount1Max, {
        accounts: {
          ownerOrDelegate: mintAuthority.publicKey,
          nftAccount: positionANftAccount,
          personalPositionState: personalPositionAState,
          ammConfig,
          poolState: poolAState,
          protocolPositionState: protocolPositionAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          bitmapLowerState: bitmapLowerAState,
          bitmapUpperState: bitmapUpperAState,
          lastObservationState: lastObservationAState,
          tokenVault0: vaultA0,
          tokenVault1: vaultA1,
          recipientTokenAccount0: feeRecipientWallet0,
          recipientTokenAccount1: feeRecipientWallet1,
          tokenProgram: TOKEN_PROGRAM_ID,
        },
        remainingAccounts: [
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ],
        signers: [mintAuthority],
      });
      console.log("collectFromTokenized delegated authority, tx: ", tx);
      const protocolPositionData =
        await program.account.procotolPositionState.fetch(
          protocolPositionAState
        );
      console.log("corePositionAState: ", protocolPositionData);
      assert.equal(protocolPositionData.tokenFeesOwed0.toNumber(), 35);
      assert.equal(protocolPositionData.tokenFeesOwed1.toNumber(), 0);

      const tokenizedPositionData =
        await program.account.personalPositionState.fetch(
          personalPositionAState
        );
      assert.equal(tokenizedPositionData.tokenFeesOwed0.toNumber(), 35);
      assert.equal(tokenizedPositionData.tokenFeesOwed1.toNumber(), 0);

      const recipientWallet0Info = await token0.getAccountInfo(
        feeRecipientWallet0
      );
      const recipientWallet1Info = await token1.getAccountInfo(
        feeRecipientWallet1
      );
      assert.equal(recipientWallet0Info.amount.toNumber(), 15);
      // assert.equal(recipientWallet1Info.amount.toNumber(), 20);
      assert.equal(recipientWallet1Info.amount.toNumber(), 0);

      const vault0Info = await token0.getAccountInfo(vaultA0);
      const vault1Info = await token1.getAccountInfo(vaultA1);
      assert.equal(vault0Info.amount.toNumber(), 99991);
      assert.equal(vault1Info.amount.toNumber(), 899453);
    });
  });

  describe("#swap_base_output_single", () => {
    it("fails if amount_in is greater than amountInMaximum", async () => {
      const amountInMaximum = new BN(100);
      const amountOut = new BN(100_000);
      const sqrtPriceLimitX32 = new BN(0);

      await expect(
        program.rpc.swap(amountOut, amountInMaximum, sqrtPriceLimitX32, false, {
          accounts: {
            signer: owner,
            ammConfig,
            poolState: poolAState,
            inputTokenAccount: minterWallet0,
            outputTokenAccount: minterWallet1,
            inputVault: vaultA0,
            outputVault: vaultA1,
            lastObservationState: lastObservationAState,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: bitmapLowerAState,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        })
      ).to.be.rejectedWith(Error);
    });

    it("performs a zero for one swap with exact output", async () => {
      let poolStateDataBefore = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price", poolStateDataBefore.sqrtPrice.toNumber());
      console.log("pool tick", poolStateDataBefore.tick);

      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      lastObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed(observationIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext),
          ],
          program.programId
        )
      )[0];

      const amountInMaximum = new BN(100_000);
      const amountOut = new BN(100_000);
      const sqrtPriceLimitX32 = new BN(0);

      console.log(
        "pool tick",
        uniPoolA.tickCurrent,
        "price",
        uniPoolA.sqrtRatioX32.toString()
      );
      const [expectedAmountIn, expectedNewPool] = await uniPoolA.getInputAmount(
        CurrencyAmount.fromRawAmount(uniToken1, amountOut.toNumber())
        // JSBI.BigInt(sqrtPriceLimitX32)
      );
      console.log(
        "expectedAmountIn: ",
        JSBI.toNumber(expectedAmountIn.numerator)
      );
      console.log("expected pool", expectedNewPool);

      let vaultBalanceA0Before = await token0.getAccountInfo(vaultA0);
      await program.methods
        .swap(amountOut, amountInMaximum, sqrtPriceLimitX32, false)
        .accounts({
          signer: owner,
          ammConfig,
          poolState: poolAState,
          inputTokenAccount: minterWallet0,
          outputTokenAccount: minterWallet1,
          inputVault: vaultA0,
          outputVault: vaultA1,
          lastObservationState: lastObservationAState,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          {
            pubkey: bitmapLowerAState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
        ])
        .rpc();

      const poolStateDataAfter = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price after", poolStateDataAfter.sqrtPrice.toNumber());
      console.log("pool tick after", poolStateDataAfter.tick);

      assert.equal(poolStateDataAfter.tick, expectedNewPool.tickCurrent);
      assert.equal(
        poolStateDataAfter.sqrtPrice.toNumber(),
        JSBI.toNumber(expectedNewPool.sqrtRatioX32)
      );

      let vaultBalanceA0After = await token0.getAccountInfo(vaultA0);
      assert.equal(
        JSBI.toNumber(expectedAmountIn.numerator),
        new Number(vaultBalanceA0After.amount.sub(vaultBalanceA0Before.amount))
      );
      uniPoolA = expectedNewPool;
    });
  });

  describe("#exact_input", () => {
    it("performs a single pool swap", async () => {
      const poolStateDataBefore = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price", poolStateDataBefore.sqrtPrice.toNumber());
      console.log("pool tick", poolStateDataBefore.tick);

      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      lastObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed(observationIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext),
          ],
          program.programId
        )
      )[0];

      const amountIn = new BN(100_000);
      const amountOutMinimum = new BN(0);
      const [expectedAmountOut, expectedNewPool, swapAccounts] =
        await uniPoolA.getOutputAmount(
          CurrencyAmount.fromRawAmount(uniToken0, amountIn.toNumber())
        );
      console.log(
        "expectedAmountOut: ",
        JSBI.toNumber(expectedAmountOut.numerator)
      );
      console.log("expected pool", expectedNewPool);

      await program.rpc.swapRouterBaseIn(
        amountIn,
        amountOutMinimum,
        Buffer.from([2]),
        {
          accounts: {
            signer: owner,
            ammConfig,
            inputTokenAccount: minterWallet0,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: poolAState,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: minterWallet1, // outputTokenAccount
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: vaultA0, // input vault
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: vaultA1, // output vault
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: lastObservationAState,
              isSigner: false,
              isWritable: true,
            },
            ...swapAccounts,
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }
      );

      const poolStateDataAfter = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price after", poolStateDataAfter.sqrtPrice.toNumber());
      console.log("pool tick after", poolStateDataAfter.tick);

      console.log("---------------poolAStateData3: ", poolStateDataAfter);
    });

    it("creates a second liquidity pool", async () => {
      await program.rpc.createPool(initialPriceX32, {
        accounts: {
          poolCreator: owner,
          ammConfig: ammConfig,
          tokenMint0: token1.publicKey,
          tokenMint1: token2.publicKey,
          feeState,
          poolState: poolBState,
          initialObservationState: initialObservationStateB,
          tokenVault0: vaultB1,
          tokenVault1: vaultB2,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
        },
      });
      console.log("second pool created");

      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolBState);

      latestObservationBState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolBState.toBuffer(),
            u16ToSeed(observationIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationBState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolBState.toBuffer(),
            u16ToSeed((observationIndex + 1) % observationCardinalityNext),
          ],
          program.programId
        )
      )[0];

      // create tick and bitmap accounts
      // can't combine with createTokenizedPosition due to size limit

      await program.rpc.createPosition(
        tickLower,
        tickUpper,
        wordPosLower,
        wordPosUpper,
        amount0Desired,
        amount1Desired,
        new BN(0),
        new BN(0),
        {
          accounts: {
            minter: owner,
            positionNftOwner: owner,
            ammConfig,
            positionNftMint: nftMintBKeypair.publicKey,
            positionNftAccount: positionBNftAccount,
            metadataAccount: metadataBAccount,
            poolState: poolBState,
            protocolPositionState: protocolPositionBState,
            tickLowerState: tickLowerBState,
            tickUpperState: tickUpperBState,
            bitmapLowerState: bitmapLowerBState,
            bitmapUpperState: bitmapUpperBState,
            tokenAccount0: minterWallet1,
            tokenAccount1: minterWallet2,
            tokenVault0: vaultB1,
            tokenVault1: vaultB2,
            lastObservationState: latestObservationBState,
            personalPositionState: personalPositionBState,
            systemProgram: SystemProgram.programId,
            rent: web3.SYSVAR_RENT_PUBKEY,
            tokenProgram: TOKEN_PROGRAM_ID,
            associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
            metadataProgram: metaplex.programs.metadata.MetadataProgram.PUBKEY,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationBState,
              isSigner: false,
              isWritable: true,
            },
          ],
          signers: [nftMintBKeypair],
        }
      );
    });

    it("perform a two pool swap", async () => {
      const poolStateDataBefore = await program.account.poolState.fetch(
        poolAState
      );
      console.log("pool price", poolStateDataBefore.sqrtPrice.toNumber());
      console.log("pool tick", poolStateDataBefore.tick);

      const tickBitmap_lower = (
        await PublicKey.findProgramAddress(
          [BITMAP_SEED, poolAState.toBuffer(), u16ToSeed(wordPosLower)],
          program.programId
        )
      )[0];
      console.log("tickBitmap_lower: ", tickBitmap_lower.toString());

      assert.equal(tickBitmap_lower.toString(), bitmapLowerAState.toString());
      const {
        observationIndex: observationAIndex,
        observationCardinalityNext: observationCardinalityANext,
      } = await program.account.poolState.fetch(poolAState);

      lastObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed(observationAIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationAState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolAState.toBuffer(),
            u16ToSeed((observationAIndex + 1) % observationCardinalityANext),
          ],
          program.programId
        )
      )[0];

      const {
        observationIndex: observationBIndex,
        observationCardinalityNext: observationCardinalityBNext,
      } = await program.account.poolState.fetch(poolBState);

      latestObservationBState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolBState.toBuffer(),
            u16ToSeed(observationBIndex),
          ],
          program.programId
        )
      )[0];

      nextObservationBState = (
        await PublicKey.findProgramAddress(
          [
            OBSERVATION_SEED,
            poolBState.toBuffer(),
            u16ToSeed((observationBIndex + 1) % observationCardinalityBNext),
          ],
          program.programId
        )
      )[0];

      let vaultBalanceA0 = await token0.getAccountInfo(vaultA0);
      let vaultBalanceA1 = await token1.getAccountInfo(vaultA1);
      let vaultBalanceB1 = await token1.getAccountInfo(vaultB1);
      let vaultBalanceB2 = await token2.getAccountInfo(vaultB2);
      console.log(
        "vault balances before",
        vaultBalanceA0.amount.toNumber(),
        vaultBalanceA1.amount.toNumber(),
        vaultBalanceB1.amount.toNumber(),
        vaultBalanceB2.amount.toNumber()
      );
      let token2AccountInfo = await token2.getAccountInfo(minterWallet2);
      console.log(
        "token 2 balance before",
        token2AccountInfo.amount.toNumber()
      );

      console.log("pool B address", poolBState.toString());

      const amountIn = new BN(100_000);
      const amountOutMinimum = new BN(0);
      await program.methods
        .swapRouterBaseIn(amountIn, amountOutMinimum, Buffer.from([2, 2]))
        .accounts({
          signer: owner,
          ammConfig,
          inputTokenAccount: minterWallet0,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          {
            pubkey: poolAState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: minterWallet1, // outputTokenAccount
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: vaultA0, // input vault
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: vaultA1, // output vault
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: lastObservationAState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: bitmapLowerAState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: nextObservationAState,
            isSigner: false,
            isWritable: true,
          },
          // second pool
          {
            pubkey: poolBState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: minterWallet2, // outputTokenAccount
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: vaultB1, // input vault
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: vaultB2, // output vault
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: latestObservationBState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: bitmapLowerBState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: tickUpperBState,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: nextObservationBState,
            isSigner: false,
            isWritable: true,
          },
        ])
        .rpc({
          skipPreflight: true,
          preflightCommitment: "processed",
          commitment: "processed",
        });

      vaultBalanceA0 = await token0.getAccountInfo(vaultA0);
      vaultBalanceA1 = await token1.getAccountInfo(vaultA1);
      vaultBalanceB1 = await token1.getAccountInfo(vaultB1);
      vaultBalanceB2 = await token2.getAccountInfo(vaultB2);
      console.log(
        "vault balances after",
        vaultBalanceA0.amount.toNumber(),
        vaultBalanceA1.amount.toNumber(),
        vaultBalanceB1.amount.toNumber(),
        vaultBalanceB2.amount.toNumber()
      );

      const poolStateDataAfter = await program.account.poolState.fetch(
        poolAState
      );
      console.log(
        "pool A price after",
        poolStateDataAfter.sqrtPrice.toNumber()
      );
      console.log("pool A tick after", poolStateDataAfter.tick);

      token2AccountInfo = await token2.getAccountInfo(minterWallet2);
      console.log("token 2 balance after", token2AccountInfo.amount.toNumber());
    });
  });

  describe("#collect_reward", () => {
    let ownerReceiveRewardTokenAccount0;
    let ownerReceiveRewardTokenAccount1;
    let ownerReceiveRewardTokenAccount2;

    it("update_reward_and_fee, fails if both amounts are set as 0", async () => {
      await program.methods
        .updateRewardInfos()
        .accounts({
          ammConfig,
          poolState: poolAState,
        })
        .remainingAccounts([])
        .rpc();
    });

    it("creates reward token accounts for owner", async () => {
      ownerReceiveRewardTokenAccount0 = await rewardToken0.createAccount(
        new Keypair().publicKey
      );
      ownerReceiveRewardTokenAccount1 = await rewardToken1.createAccount(
        new Keypair().publicKey
      );
      ownerReceiveRewardTokenAccount2 = await rewardToken2.createAccount(
        new Keypair().publicKey
      );
    });

    it("fails if not owner", async () => {
      await expect(
        program.methods
          .collectRewards()
          .accounts({
            ownerOrDelegate: notOwner.publicKey,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            poolState: poolAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .remainingAccounts([
            {
              pubkey: rewardVault0,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: ownerReceiveRewardTokenAccount0,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: rewardVault1,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: ownerReceiveRewardTokenAccount1,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: rewardVault2,
              isSigner: false,
              isWritable: true,
            },
            {
              pubkey: ownerReceiveRewardTokenAccount2,
              isSigner: false,
              isWritable: true,
            },
          ])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("colect all reward amount", async () => {
      await program.methods
        .collectRewards()
        .accounts({
          ownerOrDelegate: owner,
          nftAccount: positionANftAccount,
          protocolPositionState: protocolPositionAState,
          personalPositionState: personalPositionAState,
          poolState: poolAState,
          tickLowerState: tickLowerAState,
          tickUpperState: tickUpperAState,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .remainingAccounts([
          {
            pubkey: rewardVault0,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: ownerReceiveRewardTokenAccount0,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: rewardVault1,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: ownerReceiveRewardTokenAccount1,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: rewardVault2,
            isSigner: false,
            isWritable: true,
          },
          {
            pubkey: ownerReceiveRewardTokenAccount2,
            isSigner: false,
            isWritable: true,
          },
        ])
        .rpc();
    });
  });

  describe("#set_reward_emissions", () => {
    it("fails if not authority", async () => {
      await expect(
        program.methods
          .setRewardEmissions(0, new BN(10))
          .accounts({
            authority: notOwner.publicKey,
            ammConfig: ammConfig,
            poolState: poolAState,
          })
          .remainingAccounts([])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("fails if index overflow", async () => {
      await expect(
        program.methods
          .setRewardEmissions(3, new BN(10))
          .accounts({
            authority: owner,
            ammConfig: ammConfig,
            poolState: poolAState,
          })
          .remainingAccounts([])
          .rpc()
      ).to.be.rejectedWith(Error);
    });

    it("set reward index 0 emission less than before", async () => {
      await program.methods
        .setRewardEmissions(0, new BN(1))
        .accounts({
          authority: owner,
          ammConfig: ammConfig,
          poolState: poolAState,
        })
        .remainingAccounts([])
        .rpc();

      const poolStateData = await program.account.poolState.fetch(poolAState);

      assert.equal(
        poolStateData.rewardInfos[0].emissionsPerSecondX32.toNumber(),
        1
      );
    });

    // it("set reward index 0 emission grater than before", async () => {
    //   await program.methods
    //     .setRewardEmissions(0, new BN(1000000000000000))
    //     .accounts({
    //       authority: owner,
    //       ammConfig: ammConfig,
    //       poolState: poolAState,
    //     })
    //     .remainingAccounts([
    //       {
    //         pubkey: rewardVault0,
    //         isSigner: false,
    //         isWritable: true,
    //       },
    //       {
    //         pubkey: ownerRewardTokenAccount0,
    //         isSigner: false,
    //         isWritable: true,
    //       },
    //       {
    //         pubkey: TOKEN_PROGRAM_ID,
    //         isSigner: false,
    //         isWritable: false,
    //       },
    //     ])
    //     .rpc();

    //   const poolStateData = await program.account.poolState.fetch(poolAState);

    //   assert.equal(
    //     poolStateData.rewardInfos[0].emissionPerSecondX32.toNumber(),
    //     1000000000000000
    //   );
    // });
  });

  describe("Completely close position and deallocate ticks", () => {
    it("update observation accounts", async () => {
      const { observationIndex, observationCardinalityNext } =
        await program.account.poolState.fetch(poolAState);

      const { blockTimestamp: lastBlockTime } =
        await program.account.observationState.fetch(lastObservationAState);

      const slot = await connection.getSlot();
      const blockTimestamp = await connection.getBlockTime(slot);

      // If current observation account will expire in 3 seconds, we sleep for this time
      // before recalculating the observation states
      if (
        Math.floor(lastBlockTime / 14) == Math.floor(blockTimestamp / 14) &&
        lastBlockTime % 14 >= 11
      ) {
        await new Promise((r) => setTimeout(r, 3000));
      }
      if (Math.floor(lastBlockTime / 14) > Math.floor(blockTimestamp / 14)) {
        lastObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),

              u16ToSeed(observationIndex),
            ],
            program.programId
          )
        )[0];

        nextObservationAState = (
          await PublicKey.findProgramAddress(
            [
              OBSERVATION_SEED,
              poolAState.toBuffer(),

              u16ToSeed((observationIndex + 1) % observationCardinalityNext),
            ],
            program.programId
          )
        )[0];
      }
    });

    it("burn entire of the position liquidity as owner", async () => {
      const { liquidity } = await program.account.personalPositionState.fetch(
        personalPositionAState
      );
      console.log("liquidity in position", liquidity);
      const tx = new Transaction();
      tx.instructions = [
        program.instruction.decreaseLiquidity(liquidity, new BN(0), new BN(0), {
          accounts: {
            ownerOrDelegate: owner,
            nftAccount: positionANftAccount,
            personalPositionState: personalPositionAState,
            ammConfig,
            poolState: poolAState,
            protocolPositionState: protocolPositionAState,
            tickLowerState: tickLowerAState,
            tickUpperState: tickUpperAState,
            bitmapLowerState: bitmapLowerAState,
            bitmapUpperState: bitmapUpperAState,
            lastObservationState: lastObservationAState,
            tokenVault0: vaultA0,
            tokenVault1: vaultA1,
            recipientTokenAccount0: feeRecipientWallet0,
            recipientTokenAccount1: feeRecipientWallet1,
            tokenProgram: TOKEN_PROGRAM_ID,
          },
          remainingAccounts: [
            {
              pubkey: nextObservationAState,
              isSigner: false,
              isWritable: true,
            },
          ],
        }),
        // not support
        // program.instruction.closeTickAccount({
        //   accounts: {
        //     recipient: owner,
        //     tickState: tickLowerAState,
        //   },
        // }),
        // program.instruction.closeTickAccount({
        //   accounts: {
        //     recipient: owner,
        //     tickState: tickUpperAState,
        //   },
        // }),
      ];
      tx.recentBlockhash = (await connection.getRecentBlockhash()).blockhash;
      await anchor.getProvider().send(tx);
    });
  });
});
