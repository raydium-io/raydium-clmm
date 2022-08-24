import { web3, BN } from "@project-serum/anchor";
import { Token, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { AmmPool } from "./pool";
import {
  getTickWithPriceAndTickspacing,
  LiquidityMath,
  SqrtPriceMath,
} from "./math";
import { StateFetcher, OBSERVATION_STATE_LEN } from "./states";
import { accountExist, getAmmConfigAddress, sendTransaction } from "./utils";
import { AmmInstruction, RouterPoolParam } from "./instructions";
import {
  Connection,
  ConfirmOptions,
  PublicKey,
  Keypair,
  Signer,
  ComputeBudgetProgram,
  TransactionInstruction,
  SystemProgram,
  TransactionSignature,
} from "@solana/web3.js";
import { Context, NodeWallet } from "./base";
import Decimal from "decimal.js";

const defaultConfirmOptions: ConfirmOptions = {
  preflightCommitment: "processed",
  commitment: "processed",
  skipPreflight: true,
};

function localWallet(): Keypair {
  return Keypair.fromSecretKey(
    new Uint8Array([
      12, 14, 221, 123, 106, 110, 90, 126, 26, 140, 181, 162, 148, 212, 32, 1,
      59, 85, 4, 75, 39, 92, 134, 194, 81, 99, 237, 93, 16, 209, 25, 93, 89, 83,
      9, 155, 52, 216, 158, 126, 151, 206, 205, 63, 159, 129, 183, 145, 213,
      243, 142, 90, 227, 81, 149, 67, 240, 245, 14, 175, 230, 215, 89, 253,
    ])
  );
}

async function getContext(programId: PublicKey, wallet: Keypair, url: string) {
  const connection = new Connection(url, defaultConfirmOptions.commitment);
  return new Context(
    connection,
    NodeWallet.fromSecretKey(wallet),
    programId,
    defaultConfirmOptions
  );
}

export async function main() {
  const owner = localWallet();
  console.log("owner: ", owner.publicKey.toString());
  const programId = new PublicKey(
    "DevadyVYwyiMQikvjkFYmiaobLNaGsJJbgsEL1Rfp3zK"
  );
  const url = "https://api.devnet.solana.com";
  // const url = "http://127.0.0.1:8899";
  const ctx = await getContext(programId, owner, url);
  const stateFetcher = new StateFetcher(ctx.program);

  // parper token and associated token account
  const mintAuthority = new Keypair();
  const [
    { token0, ownerToken0Account },
    { token1, ownerToken1Account },
    { token2, ownerToken2Account },
  ] = await createTokenMintAndAssociatedTokenAccount(ctx, owner, mintAuthority);

  // First, create config account
  // Admin key is hardcode, if you want create a amm config with a keypair, must change it in contract.
  const admin = owner;
  const ammConfigAddress = await createAmmConfig(
    ctx,
    admin,
    0,
    10,
    1000,
    25000
  );

  // Second, create a pool
  const [poolAAddress, poolTx] = await createPool(
    ctx,
    owner,
    ammConfigAddress,
    token0.publicKey,
    token1.publicKey,
    new Decimal(1),
    6,
    6
  );
  console.log("createPool tx:", poolTx);


  const ammPoolA = new AmmPool(
    ctx,
    poolAAddress,
    stateFetcher
  );

  await ammPoolA.loadPoolState();

  // console.log(SqrtPriceMath.sqrtPriceX64ToPrice(SqrtPriceMath.getSqrtPriceX64FromTick(-20)).toString())
  // console.log(SqrtPriceMath.sqrtPriceX64ToPrice(SqrtPriceMath.getSqrtPriceX64FromTick(20)).toString())
  // Open position with created pool
  const [positionAccountAddress, positionTx] = await createPersonalPosition(
    ctx,
    owner,
    ammPoolA,
    new Decimal("0.99800209846088566961"),
    new Decimal("1.0020019011404840582"),
    new BN(1_000_000),
    new BN(1_000_000),
    0.005
  );
  console.log("createPersonalPosition tx:", positionTx);

  // Increase liquitidity with existed position
  let tx = await increaseLiquidity(
    ctx,
    owner,
    ammPoolA,
    positionAccountAddress,
    new BN(1_000_000),
    new BN(1_000_000),
    0.005
  );
  console.log("increaseLiquidity tx:", tx);

  // Decrease liquitidity with existed position
  tx = await decreaseLiquidity(
    ctx,
    owner,
    ammPoolA,
    positionAccountAddress,
    new BN(1_000_000),
    new BN(1_000_000),
    0.005,
    defaultConfirmOptions
  );
  console.log("decreaseLiquidity tx:", tx);

  // swapBaseIn with limit price
  let limitPrice = ammPoolA.tokenPrice().sub(new Decimal("0.0000002"));
  // because open position and add liquidity to the pool, we should load tickArray cache data
  await ammPoolA.loadPoolState();

  tx = await swapBaseIn(
    ctx,
    owner,
    ammPoolA,
    ownerToken0Account,
    ownerToken1Account,
    token0.publicKey,
    new BN(100_000),
    0.005,
    limitPrice
  );
  console.log("swapBaseIn with limit price tx:", tx);

  await ammPoolA.loadPoolState();
  // swapBaseIn without limit price
  tx = await swapBaseIn(
    ctx,
    owner,
    ammPoolA,
    ownerToken0Account,
    ownerToken1Account,
    token0.publicKey,
    new BN(100_000),
    0.005
  );
  console.log("swapBaseIn without limit price tx:", tx);

  await ammPoolA.loadPoolState();

  tx = await swapBaseOut(
    ctx,
    owner,
    ammPoolA,
    ownerToken0Account,
    ownerToken1Account,
    token1.publicKey,
    new BN(100_000),
    0.005
  );
  console.log("swapBaseOut tx:", tx);

  await ammPoolA.loadPoolState();
  // only collect fee
  tx = await decreaseLiquidity(
    ctx,
    owner,
    ammPoolA,
    positionAccountAddress,
    new BN(0),
    new BN(0)
  );
  console.log("collect fee tx:", tx);

  await ammPoolA.loadPoolState();
  // create a second pool for swap router
  const [poolBAddress, poolBTx] = await createPool(
    ctx,
    owner,
    ammConfigAddress,
    token1.publicKey,
    token2.publicKey,
    new Decimal(1),
    6,
    6
  );

  const ammPoolB = new AmmPool(
    ctx,
    poolBAddress,
    stateFetcher
  );

  await ammPoolB.loadPoolState();

  // Open position with pool B
  const [positionBccountAddress, positionBTx] = await createPersonalPosition(
    ctx,
    owner,
    ammPoolB,
    new Decimal("0.99800209846088566961"),
    new Decimal("1.0020019011404840582"),
    new BN(1_000_000),
    new BN(1_000_000)
  );
  console.log("open second position with pool B, tx:", positionBTx);

  await ammPoolB.loadPoolState();
  // because open position and add liquidity to the pool, we should reload tickArray cache data
  tx = await swapRouterBaseIn(
    ctx,
    owner,
    {
      ammPool: ammPoolA,
      inputTokenMint: token0.publicKey,
    },
    [ammPoolB],
    new BN(100_000),
    0.02
  );
  console.log("swapRouterBaseIn tx:", tx);
}

async function createTokenMintAndAssociatedTokenAccount(
  ctx: Context,
  payer: Signer,
  mintAuthority: Signer
) {
  let ixs: TransactionInstruction[] = [];
  ixs.push(
    web3.SystemProgram.transfer({
      fromPubkey: payer.publicKey,
      toPubkey: mintAuthority.publicKey,
      lamports: web3.LAMPORTS_PER_SOL,
    })
  );
  await sendTransaction(ctx.connection, ixs, [payer]);
  let tokenArray: Token[] = [];
  tokenArray.push(
    await Token.createMint(
      ctx.connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    )
  );
  tokenArray.push(
    await Token.createMint(
      ctx.connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    )
  );
  tokenArray.push(
    await Token.createMint(
      ctx.connection,
      mintAuthority,
      mintAuthority.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    )
  );

  tokenArray.sort(function (x, y) {
    if (x.publicKey < y.publicKey) {
      return -1;
    }
    if (x.publicKey > y.publicKey) {
      return 1;
    }
    return 0;
  });

  const token0 = tokenArray[0];
  const token1 = tokenArray[1];
  const token2 = tokenArray[2];

  console.log("Token 0", token0.publicKey.toString());
  console.log("Token 1", token1.publicKey.toString());
  console.log("Token 2", token2.publicKey.toString());

  const ownerToken0Account = await token0.createAssociatedTokenAccount(
    payer.publicKey
  );
  const ownerToken1Account = await token1.createAssociatedTokenAccount(
    payer.publicKey
  );
  const ownerToken2Account = await token2.createAssociatedTokenAccount(
    payer.publicKey
  );
  await token0.mintTo(ownerToken0Account, mintAuthority, [], 100_000_000);
  await token1.mintTo(ownerToken1Account, mintAuthority, [], 100_000_000);
  await token2.mintTo(ownerToken2Account, mintAuthority, [], 100_000_000);

  console.log("ownerToken0Account key: ", ownerToken0Account.toString());
  console.log("ownerToken1Account key: ", ownerToken1Account.toString());
  console.log("ownerToken2Account key: ", ownerToken2Account.toString());

  return [
    { token0, ownerToken0Account },
    { token1, ownerToken1Account },
    { token2, ownerToken2Account },
  ];
}

async function createAmmConfig(
  ctx: Context,
  owner: Signer,
  index: number,
  tickSpacing: number,
  globalFeeRate: number,
  protocolFeeRate: number,
  confirmOptions?: ConfirmOptions
): Promise<PublicKey> {
  // Only for test, you needn't do it
  const [address1, _] = await getAmmConfigAddress(0, ctx.program.programId);
  if (await accountExist(ctx.connection, address1)) {
    return address1;
  }
  // Build instrcution
  const [address, ix] = await AmmInstruction.createAmmConfig(
    ctx,
    owner.publicKey,
    index,
    tickSpacing,
    globalFeeRate,
    protocolFeeRate
  );
  const tx = await sendTransaction(
    ctx.provider.connection,
    [ix],
    [owner],
    confirmOptions
  );
  console.log("init amm config tx: ", tx);
  return address;
}

async function createPool(
  ctx: Context,
  owner: Signer,
  ammConfig: PublicKey,
  token0Mint: PublicKey,
  token1Mint: PublicKey,
  initialPrice: Decimal,
  tokenMint0Decimals: number,
  tokenMint1Decimals: number,
  confirmOptions?: ConfirmOptions
): Promise<[PublicKey, TransactionSignature]> {
  const observation = new Keypair();
  const createObvIx = SystemProgram.createAccount({
    fromPubkey: owner.publicKey,
    newAccountPubkey: observation.publicKey,
    lamports: await ctx.provider.connection.getMinimumBalanceForRentExemption(
      OBSERVATION_STATE_LEN
    ),
    space: OBSERVATION_STATE_LEN,
    programId: ctx.program.programId,
  });
  const [address, ixs] = await AmmInstruction.createPool(
    ctx,
    {
      poolCreator: owner.publicKey,
      ammConfig: ammConfig,
      tokenMint0: token0Mint,
      tokenMint1: token1Mint,
      observation: observation.publicKey,
    },
    initialPrice,
    tokenMint0Decimals,
    tokenMint1Decimals
  );

  const tx = await sendTransaction(
    ctx.provider.connection,
    [createObvIx, ixs],
    [owner, observation],
    confirmOptions
  );
  return [address, tx];
}

async function createPersonalPosition(
  ctx: Context,
  owner: Signer,
  ammPool: AmmPool,
  priceLower: Decimal,
  priceUpper: Decimal,
  token0Amount: BN,
  token1Amount: BN,
  amountSlippage?: number,
  confirmOptions?: ConfirmOptions
): Promise<[PublicKey, TransactionSignature]> {
  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });

  const tickLower = ammPool.getRoundingTickWithPrice(priceLower);
  const tickUpper = ammPool.getRoundingTickWithPrice(priceUpper);

  const liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
    ammPool.poolState.sqrtPriceX64,
    SqrtPriceMath.getSqrtPriceX64FromTick(tickLower),
    SqrtPriceMath.getSqrtPriceX64FromTick(tickUpper),
    token0Amount,
    token1Amount
  );
  const nftMintAKeypair = new Keypair();

  let instructions: TransactionInstruction[] = [
    additionalComputeBudgetInstruction,
  ];
  let signers: Signer[] = [owner, nftMintAKeypair];

  const {
    personalPosition,
    instructions: ixs,
    signers: signer,
  } = await AmmInstruction.openPosition(
    {
      payer: owner.publicKey,
      positionNftOwner: owner.publicKey,
      positionNftMint: nftMintAKeypair.publicKey,
    },
    ammPool,
    tickLower,
    tickUpper,
    liquidity,
    amountSlippage
  );
  instructions.push(...ixs);
  signers.push(...signer);

  const tx = await sendTransaction(
    ctx.provider.connection,
    instructions,
    signers,
    confirmOptions
  );
  return [personalPosition, tx];
}

async function increaseLiquidity(
  ctx: Context,
  owner: Signer,
  ammPool: AmmPool,
  personalPosition: PublicKey,
  token0AmountDesired: BN,
  token1AmountDesired: BN,
  amountSlippage?: number,
  confirmOptions?: ConfirmOptions
): Promise<TransactionSignature> {
  const personalPositionData =
    await ammPool.stateFetcher.getPersonalPositionState(personalPosition);

  const liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
    ammPool.poolState.sqrtPriceX64,
    SqrtPriceMath.getSqrtPriceX64FromTick(personalPositionData.tickLowerIndex),
    SqrtPriceMath.getSqrtPriceX64FromTick(personalPositionData.tickUpperIndex),
    token0AmountDesired,
    token1AmountDesired
  );

  let instructions: TransactionInstruction[] = [];
  let signers: Signer[] = [owner];

  const { instructions: ixs, signers: signer } =
    await AmmInstruction.increaseLiquidity(
      {
        positionNftOwner: owner.publicKey,
      },
      ammPool,
      personalPositionData,
      liquidity,
      amountSlippage
    );
  instructions.push(...ixs);
  signers.push(...signer);

  return await sendTransaction(
    ctx.connection,
    instructions,
    signers,
    confirmOptions
  );
}

async function decreaseLiquidity(
  ctx: Context,
  owner: Signer,
  ammPool: AmmPool,
  personalPosition: PublicKey,
  token0AmountDesired: BN,
  token1AmountDesired: BN,
  amountSlippage?: number,
  confirmOptions?: ConfirmOptions
): Promise<TransactionSignature> {
  const personalPositionData =
    await ammPool.stateFetcher.getPersonalPositionState(personalPosition);
  let instructions: TransactionInstruction[] = [];
  let signers: Signer[] = [owner];

  const { instructions: ixs, signers: signer } =
    await AmmInstruction.decreaseLiquidityWithInputAmount(
      {
        positionNftOwner: owner.publicKey,
      },
      ammPool,
      personalPositionData,
      token0AmountDesired,
      token1AmountDesired,
      amountSlippage
    );
  instructions.push(...ixs);
  signers.push(...signer);
  return await sendTransaction(
    ctx.connection,
    instructions,
    signers,
    confirmOptions
  );
}

async function swapBaseIn(
  ctx: Context,
  owner: Signer,
  ammPool: AmmPool,
  ownerToken0Account: PublicKey,
  ownerToken1Account: PublicKey,
  inputTokenMint: PublicKey,
  amountIn: BN,
  amountOutSlippage?: number,
  priceLimit?: Decimal
): Promise<TransactionSignature> {
  let instructions: TransactionInstruction[] = [];
  let signers: Signer[] = [owner];

  const { instructions: ixs, signers: signer } =
    await AmmInstruction.swapBaseIn(
      {
        payer: owner.publicKey,
      },
      ammPool,
      inputTokenMint,
      amountIn,
      amountOutSlippage,
      priceLimit
    );
  instructions.push(...ixs);
  signers.push(...signer);
  return await sendTransaction(
    ctx.connection,
    instructions,
    signers,
    defaultConfirmOptions
  );
}

async function swapBaseOut(
  ctx: Context,
  owner: Signer,
  ammPool: AmmPool,
  ownerToken0Account: PublicKey,
  ownerToken1Account: PublicKey,
  outputTokenMint: PublicKey,
  amountOut: BN,
  amountInSlippage?: number,
  priceLimit?: Decimal
): Promise<TransactionSignature> {
  let instructions: TransactionInstruction[] = [];
  let signers: Signer[] = [owner];

  const { instructions: ixs, signers: signer } =
    await AmmInstruction.swapBaseOut(
      {
        payer: owner.publicKey,
      },
      ammPool,
      outputTokenMint,
      amountOut,
      amountInSlippage,
      priceLimit
    );
  instructions.push(...ixs);
  signers.push(...signer);
  return await sendTransaction(ctx.connection, instructions, signers);
}

async function swapRouterBaseIn(
  ctx: Context,
  owner: Signer,
  firstPoolParam: RouterPoolParam,
  remainRouterPools: AmmPool[],
  amountIn: BN,
  amountOutSlippage?: number
): Promise<TransactionSignature> {
  const additionalComputeBudgetInstruction = ComputeBudgetProgram.requestUnits({
    units: 400000,
    additionalFee: 0,
  });

  let instructions: TransactionInstruction[] = [
    additionalComputeBudgetInstruction,
  ];
  let signers: Signer[] = [owner];

  const { instructions: ixs, signers: signer } =
    await AmmInstruction.swapRouterBaseIn(
      owner.publicKey,
      firstPoolParam,
      remainRouterPools,
      amountIn,
      amountOutSlippage
    );
  instructions.push(...ixs);
  signers.push(...signer);

  return await sendTransaction(
    ctx.connection,
    instructions,
    signers,
    defaultConfirmOptions
  );
}

main();
