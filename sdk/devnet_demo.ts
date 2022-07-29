import { web3, BN } from "@project-serum/anchor";
import * as metaplex from "@metaplex/js";
import {
  Token,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import * as chai from "chai";
import chaiAsPromised from "chai-as-promised";
chai.use(chaiAsPromised);
import { AmmPool, CacheDataProviderImpl } from "./pool";
import { SqrtPriceMath } from "./math";
import { StateFetcher } from "./states";

import {
  getAmmConfigAddress,
  getPoolAddress,
  getPersonalPositionAddress,
  sendTransaction,
} from "./utils";

import {
  increaseLiquidity,
  decreaseLiquidity,
  collectFee,
  openPosition,
  swapBaseIn,
  swapBaseOut,
  swapRouterBaseIn,
} from "./instructions";

const {
  metadata: { Metadata },
} = metaplex.programs;

import {
  Connection,
  ConfirmOptions,
  PublicKey,
  Keypair,
  ComputeBudgetProgram,
} from "@solana/web3.js";
import { Context, NodeWallet } from "./base";

function localWallet(): Keypair {
  const payer = Keypair.fromSecretKey(
    Buffer.from(
      JSON.parse(
        require("fs").readFileSync("./keypair.json", {
          encoding: "utf-8",
        })
      )
    )
  );
  return payer;
}

describe("test with given pool", async () => {
  return;
  console.log(SqrtPriceMath.getSqrtPriceX64FromTick(0).toString());
  console.log(SqrtPriceMath.getSqrtPriceX64FromTick(1).toString());

  const programId = new PublicKey(
    "DEvgL6xhASESaETKSepvprNLKGXfkhVEpkhfASRFWZRb"
  );

  const url = "https://api.devnet.solana.com";
  const confirmOptions: ConfirmOptions = {
    preflightCommitment: "processed",
    commitment: "processed",
    skipPreflight: true,
  };
  const connection = new Connection(url, confirmOptions.commitment);
  console.log("new connection success");
  const wallet = localWallet();
  const walletPubkey = wallet.publicKey;
  console.log("wallet address: ", walletPubkey.toString());

  const ctx = new Context(
    connection,
    NodeWallet.fromSecretKey(localWallet()),
    programId,
    confirmOptions
  );
  const program = ctx.program;

  const ownerKeyPair = wallet;
  const owner = ownerKeyPair.publicKey;
  console.log("owner address: ", owner.toString());

  const stateFetcher = new StateFetcher(program);

  // find amm config address
  const [ammConfig, ammConfigBump] = await getAmmConfigAddress(
    0,
    program.programId
  );

  let token0: Token = new Token(
    connection,
    new PublicKey("2SiSpNowr7zUv5ZJHuzHszskQNaskWsNukhivCtuVLHo"),
    TOKEN_PROGRAM_ID,
    wallet
  );
  let token1: Token = new Token(
    connection,
    new PublicKey("GfmdKWR1KrttDsQkJfwtXovZw9bUBHYkPAEwB6wZqQvJ"),
    TOKEN_PROGRAM_ID,
    wallet
  );
  let token2: Token = new Token(
    connection,
    new PublicKey("J4bbhktCKDrXEynbCzF2QejWZFBWazWxBge5MPBFmsAD"),
    TOKEN_PROGRAM_ID,
    wallet
  );

  let ammPoolA: AmmPool;
  let ammPoolB: AmmPool;

  let poolAState: web3.PublicKey;
  let poolAStateBump: number;
  let poolBState: web3.PublicKey;
  let poolBStateBump: number;

  let ownerToken0Account: web3.PublicKey;
  let ownerToken1Account: web3.PublicKey;
  let ownerToken2Account: web3.PublicKey;

  const nftMintAKeypair = new Keypair();
  const nftMintBKeypair = new Keypair();

  let personalPositionAState: web3.PublicKey;
  let personalPositionABump: number;
  let personalPositionBState: web3.PublicKey;
  let personalPositionBBump: number;

  it("creates token accounts for position minter and airdrops to them", async () => {
    ownerToken0Account = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token0.publicKey,
      owner
    );
    ownerToken1Account = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token1.publicKey,
      owner
    );
    ownerToken2Account = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      token2.publicKey,
      owner
    );
  });

  it("derive pool address", async () => {
    [poolAState, poolAStateBump] = await getPoolAddress(
      ammConfig,
      token0.publicKey,
      token1.publicKey,
      program.programId
    );
    console.log("got poolA address", poolAState.toString());

    [poolBState, poolBStateBump] = await getPoolAddress(
      ammConfig,
      token0.publicKey,
      token2.publicKey,
      program.programId
    );
    console.log("got poolB address", poolBState.toString());
  });

  it("find program accounts addresses for position creation", async () => {
    [personalPositionAState, personalPositionABump] =
      await getPersonalPositionAddress(
        nftMintAKeypair.publicKey,
        program.programId
      );
    console.log(
      "personalPositionAState key: ",
      personalPositionAState.toString()
    );
    [personalPositionBState, personalPositionBBump] =
      await getPersonalPositionAddress(
        nftMintBKeypair.publicKey,
        program.programId
      );
    console.log(
      "personalPositionBState key: ",
      personalPositionBState.toString()
    );
  });

  describe("#create_personal_position", () => {
    it("open personal position", async () => {
      const cacheDataProvider = new CacheDataProviderImpl(program, poolAState);
      const ammConfigData = await stateFetcher.getAmmConfig(ammConfig);
      const poolStateAData = await stateFetcher.getPoolState(poolAState);
      await cacheDataProvider.loadTickArrayCache(
        poolStateAData.tickCurrent,
        poolStateAData.tickSpacing
      );

      ammPoolA = new AmmPool(
        ctx,
        poolAState,
        poolStateAData,
        ammConfigData,
        stateFetcher,
        cacheDataProvider
      );

      const additionalComputeBudgetInstruction =
        ComputeBudgetProgram.requestUnits({
          units: 400000,
          additionalFee: 0,
        });

      const [_, openIx] = await openPosition(
        {
          payer: owner,
          positionNftOwner: owner,
          positionNftMint: nftMintAKeypair.publicKey,
          token0Account: ownerToken0Account,
          token1Account: ownerToken1Account,
        },
        ammPoolA,
        -20,
        20,
        new BN(1_000_000),
        new BN(1_000_000)
      );

      const tx = await sendTransaction(
        connection,
        [additionalComputeBudgetInstruction, openIx],
        [ownerKeyPair, nftMintAKeypair],
        confirmOptions
      );

      console.log("create position, tx:", tx);
    });
  });

  describe("#increase_liquidity", () => {
    it("Add token to the position", async () => {
      const personalPositionData = await stateFetcher.getPersonalPositionState(
        personalPositionAState
      );

      const ix = await increaseLiquidity(
        {
          positionNftOwner: owner,
          token0Account: ownerToken0Account,
          token1Account: ownerToken1Account,
        },
        ammPoolA,
        personalPositionData,
        new BN(1_000_000),
        new BN(1_000_000)
      );
      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );

      console.log("increaseLiquidity tx: ", tx);
    });
  });

  describe("#decrease_liquidity", () => {
    it("burn liquidity as owner", async () => {
      const personalPositionData = await stateFetcher.getPersonalPositionState(
        personalPositionAState
      );

      const ix = await decreaseLiquidity(
        {
          positionNftOwner: owner,
          token0Account: ownerToken0Account,
          token1Account: ownerToken1Account,
        },
        ammPoolA,
        personalPositionData,
        personalPositionData.liquidity.divn(2)
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("tx:", tx);
    });
  });

  describe("#swap_base_input_single", () => {
    it("zero to one swap with a limit price", async () => {
      await ammPoolA.reload(true);
      const amountIn = new BN(100_000);
      const sqrtPriceLimitX64 = ammPoolA.poolState.sqrtPriceX64.sub(
        new BN(1000000000)
      );

      const ix = await swapBaseIn(
        {
          payer: owner,
          inputTokenAccount: ownerToken0Account,
          outputTokenAccount: ownerToken1Account,
        },
        ammPoolA,
        token0.publicKey,
        amountIn,
        0,
        sqrtPriceLimitX64
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("swap tx:", tx);
    });

    it("zero to one swap without a limit price", async () => {
      const amountIn = new BN(100_000);
      const ix = await swapBaseIn(
        {
          payer: owner,
          inputTokenAccount: ownerToken0Account,
          outputTokenAccount: ownerToken1Account,
        },
        ammPoolA,
        token0.publicKey,
        amountIn
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("swap tx:", tx);
    });
  });

  describe("#swap_base_output_single", () => {
    it("zero for one swap base output", async () => {
      const amountOut = new BN(100_000);
      await ammPoolA.reload(true);
      const ix = await swapBaseOut(
        {
          payer: owner,
          inputTokenAccount: ownerToken0Account,
          outputTokenAccount: ownerToken1Account,
        },
        ammPoolA,
        token1.publicKey,
        amountOut
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("swap tx:", tx);
    });
  });

  describe("#swap_router_base_in", () => {
    it("open second pool position", async () => {
      const cacheDataProvider = new CacheDataProviderImpl(program, poolBState);
      const poolStateBData = await stateFetcher.getPoolState(poolBState);
      const ammConfigData = await stateFetcher.getAmmConfig(ammConfig);

      ammPoolB = new AmmPool(
        ctx,
        poolBState,
        poolStateBData,
        ammConfigData,
        stateFetcher,
        cacheDataProvider
      );
      console.log(poolStateBData);
      const additionalComputeBudgetInstruction =
        ComputeBudgetProgram.requestUnits({
          units: 400000,
          additionalFee: 0,
        });

      const [_, openIx] = await openPosition(
        {
          payer: owner,
          positionNftOwner: owner,
          positionNftMint: nftMintBKeypair.publicKey,
          token0Account: ownerToken0Account,
          token1Account: ownerToken2Account,
        },
        ammPoolB,
        -120,
        120,
        new BN(1_000_000),
        new BN(1_000_000)
      );

      const tx = await sendTransaction(
        connection,
        [additionalComputeBudgetInstruction, openIx],
        [ownerKeyPair, nftMintBKeypair],
        confirmOptions
      );
      console.log("seconde position:", tx);
    });

    it("router two pool swap", async () => {
      console.log("token1.publicKey:", token1.publicKey.toString());
      const ix = await swapRouterBaseIn(
        owner,
        {
          ammPool: ammPoolA,
          inputTokenMint: token1.publicKey,
          inputTokenAccount: ownerToken1Account,
          outputTokenAccount: ownerToken0Account,
        },
        [
          {
            ammPool: ammPoolB,
            outputTokenAccount: ownerToken2Account,
          },
        ],
        new BN(100_000),
        0.01
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("tx:", tx);
    });
  });

  describe("#collect_fee", () => {
    it("collect fee as owner", async () => {
      const amount0Max = new BN(10);
      const amount1Max = new BN(10);

      const personalPositionData = await stateFetcher.getPersonalPositionState(
        personalPositionAState
      );
      const ix = await collectFee(
        {
          positionNftOwner: owner,
          token0Account: ownerToken0Account,
          token1Account: ownerToken1Account,
        },
        ammPoolA,
        personalPositionData,
        amount0Max,
        amount1Max
      );

      const tx = await sendTransaction(
        connection,
        [ix],
        [ownerKeyPair],
        confirmOptions
      );
      console.log("tx:", tx);
    });
  });
});
