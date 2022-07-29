import { Program, BN } from "@project-serum/anchor";
import {
  Connection,
  ConfirmOptions,
  PublicKey,
  Keypair,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  ComputeBudgetProgram,
  TransactionInstruction,
  AccountMeta,
} from "@solana/web3.js";

import { programs } from "@metaplex/js";
import common from "mocha/lib/interfaces/common";
import { getArrayStartIndex } from "../entities";
import {
  SqrtPriceMath,
  LiquidityMath,
  MIN_SQRT_PRICE_X64,
  MAX_SQRT_PRICE_X64,
} from "../math";
import {
  PoolState,
  PositionState,
  ObservationState,
  AmmConfig,
  PositionRewardInfo,
  RewardInfo,
} from "../states";

import {
  accountExist,
  getAmmConfigAddress,
  getPoolAddress,
  getPoolVaultAddress,
  getObservationAddress,
  getProtocolPositionAddress,
  getNftMetadataAddress,
  getPersonalPositionAddress,
  sleep,
  sendTransaction,
  getTickArrayAddress,
} from "../utils";

import {
  Token,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

import {
  openPositionInstruction,
  createPoolInstruction,
  increaseLiquidityInstruction,
  decreaseLiquidityInstruction,
  collectFeeInstruction,
  swapInstruction,
  swapRouterBaseInInstruction,
  createAmmConfigInstruction,
} from "./";

import { AmmPool, CacheDataProviderImpl } from "../pool";
import { Context } from "../base";
import Decimal from "decimal.js";

const defaultSlippage = 0.5; // 0.5%

export type OpenPositionAccounts = {
  payer: PublicKey;
  positionNftOwner: PublicKey;
  positionNftMint: PublicKey;
  token0Account: PublicKey;
  token1Account: PublicKey;
};

export type LiquidityChangeAccounts = {
  positionNftOwner: PublicKey;
  token0Account: PublicKey;
  token1Account: PublicKey;
};

export type SwapAccounts = {
  payer: PublicKey;
  inputTokenAccount: PublicKey;
  outputTokenAccount: PublicKey;
};

export async function createAmmConfig(
  ctx: Context,
  owner: PublicKey,
  index: number,
  tickSpacing: number,
  globalFeeRate: number,
  protocolFeeRate: number
): Promise<[PublicKey, TransactionInstruction]> {
  const [address, _] = await getAmmConfigAddress(index, ctx.program.programId);
  return [
    address,
    await createAmmConfigInstruction(
      ctx.program,
      index,
      tickSpacing,
      globalFeeRate,
      protocolFeeRate,
      {
        owner: owner,
        ammConfig: address,
        systemProgram: SystemProgram.programId,
      }
    ),
  ];
}

type CreatePoolAccounts = {
  poolCreator: PublicKey;
  ammConfig: PublicKey;
  tokenMint0: PublicKey;
  tokenMint1: PublicKey;
  observation: PublicKey;
};

export async function createPool(
  ctx: Context,
  accounts: CreatePoolAccounts,
  initialPrice: Decimal
): Promise<[PublicKey, TransactionInstruction]> {
  if (accounts.tokenMint0 >= accounts.tokenMint1) {
    let tmp = accounts.tokenMint0;
    accounts.tokenMint0 = accounts.tokenMint1;
    accounts.tokenMint1 = tmp;
  }
  const [poolAddres, _bump1] = await getPoolAddress(
    accounts.ammConfig,
    accounts.tokenMint0,
    accounts.tokenMint1,
    ctx.program.programId
  );
  const [vault0, _bump2] = await getPoolVaultAddress(
    poolAddres,
    accounts.tokenMint0,
    ctx.program.programId
  );
  const [vault1, _bump3] = await getPoolVaultAddress(
    poolAddres,
    accounts.tokenMint1,
    ctx.program.programId
  );

  const initialPriceX64 = SqrtPriceMath.priceToSqrtPriceX64(initialPrice);
  const creatPoolIx = await createPoolInstruction(
    ctx.program,
    initialPriceX64,
    {
      poolCreator: accounts.poolCreator,
      ammConfig: accounts.ammConfig,
      tokenMint0: accounts.tokenMint0,
      tokenMint1: accounts.tokenMint1,
      poolState: poolAddres,
      observationState: accounts.observation,
      tokenVault0: vault0,
      tokenVault1: vault1,
      systemProgram: SystemProgram.programId,
      rent: SYSVAR_RENT_PUBKEY,
      tokenProgram: TOKEN_PROGRAM_ID,
    }
  );

  return [poolAddres, creatPoolIx];
}

export async function openPositionWithPrice(
  accounts: OpenPositionAccounts,
  ammPool: AmmPool,
  priceLower: Decimal,
  priceUpper: Decimal,
  token0Amount: BN,
  token1Amount: BN,
  token0AmountSlippage?: number,
  token1AmountSlippage?: number
): Promise<[PublicKey, TransactionInstruction]> {
  const tickLower = SqrtPriceMath.getTickFromPrice(priceLower);
  const tickUpper = SqrtPriceMath.getTickFromPrice(priceUpper);

  return openPosition(
    accounts,
    ammPool,
    tickLower,
    tickUpper,
    token0Amount,
    token1Amount,
    token0AmountSlippage,
    token1AmountSlippage
  );
}

export async function openPosition(
  accounts: OpenPositionAccounts,
  ammPool: AmmPool,
  tickLowerIndex: number,
  tickUpperIndex: number,
  token0Amount: BN,
  token1Amount: BN,
  token0AmountSlippage?: number,
  token1AmountSlippage?: number
): Promise<[PublicKey, TransactionInstruction]> {
  const poolState = ammPool.poolState;
  const ctx = ammPool.ctx;

  let amount0Min: BN = new BN(0);
  let amount1Min: BN = new BN(0);
  if (token0AmountSlippage !== undefined) {
    amount0Min = token0Amount.muln(1 - token0AmountSlippage);
  }
  if (token1AmountSlippage !== undefined) {
    amount1Min = token1Amount.muln(1 - token1AmountSlippage);
  }

  // prepare tickArray
  const tickArrayLowerStartIndex = getArrayStartIndex(
    tickLowerIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayLower] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayLowerStartIndex
  );
  const tickArrayUpperStartIndex = getArrayStartIndex(
    tickUpperIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayUpper] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayUpperStartIndex
  );

  const positionANftAccount = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    accounts.positionNftMint,
    accounts.positionNftOwner
  );

  const metadataAccount = (
    await getNftMetadataAddress(accounts.positionNftMint)
  )[0];

  const [personalPosition] = await getPersonalPositionAddress(
    accounts.positionNftMint,
    ctx.program.programId
  );

  const [protocolPosition] = await getProtocolPositionAddress(
    ammPool.address,
    ctx.program.programId,
    tickLowerIndex,
    tickUpperIndex
  );

  return [
    personalPosition,
    await openPositionInstruction(
      ctx.program,
      {
        tickLowerIndex,
        tickUpperIndex,
        tickArrayLowerStartIndex: tickArrayLowerStartIndex,
        tickArrayUpperStartIndex: tickArrayUpperStartIndex,
        amount0Desired: token0Amount,
        amount1Desired: token1Amount,
        amount0Min,
        amount1Min,
      },
      {
        payer: accounts.payer,
        positionNftOwner: accounts.positionNftOwner,
        ammConfig: poolState.ammConfig,
        positionNftMint: accounts.positionNftMint,
        positionNftAccount: positionANftAccount,
        metadataAccount,
        poolState: ammPool.address,
        protocolPosition,
        tickArrayLower,
        tickArrayUpper,
        tokenAccount0: accounts.token0Account,
        tokenAccount1: accounts.token1Account,
        tokenVault0: poolState.tokenVault0,
        tokenVault1: poolState.tokenVault1,
        personalPosition,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: programs.metadata.MetadataProgram.PUBKEY,
      }
    ),
  ];
}

export async function increaseLiquidity(
  accounts: LiquidityChangeAccounts,
  ammPool: AmmPool,
  positionState: PositionState,
  token0Amount: BN,
  token1Amount: BN,
  token0AmountSlippage?: number,
  token1AmountSlippage?: number
): Promise<TransactionInstruction> {
  const poolState = ammPool.poolState;
  const ctx = ammPool.ctx;
  const tickLowerIndex = positionState.tickLowerIndex;
  const tickUpperIndex = positionState.tickUpperIndex;

  let amount0Min: BN = new BN(0);
  let amount1Min: BN = new BN(0);
  if (token0AmountSlippage !== undefined) {
    amount0Min = token0Amount.muln(1 - token0AmountSlippage);
  }
  if (token1AmountSlippage !== undefined) {
    amount1Min = token1Amount.muln(1 - token1AmountSlippage);
  }

  // prepare tickArray
  const tickArrayLowerStartIndex = getArrayStartIndex(
    tickLowerIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayLower] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayLowerStartIndex
  );
  const tickArrayUpperStartIndex = getArrayStartIndex(
    tickUpperIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayUpper] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayUpperStartIndex
  );

  const positionANftAccount = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    positionState.nftMint,
    accounts.positionNftOwner
  );

  const [personalPosition] = await getPersonalPositionAddress(
    positionState.nftMint,
    ctx.program.programId
  );

  const [protocolPosition] = await getProtocolPositionAddress(
    ammPool.address,
    ctx.program.programId,
    tickLowerIndex,
    tickUpperIndex
  );

  return await increaseLiquidityInstruction(
    ctx.program,
    {
      amount0Desired: token0Amount,
      amount1Desired: token1Amount,
      amount0Min,
      amount1Min,
    },
    {
      nftOwner: accounts.positionNftOwner,
      ammConfig: poolState.ammConfig,
      nftAccount: positionANftAccount,
      poolState: ammPool.address,
      protocolPosition,
      tickArrayLower,
      tickArrayUpper,
      tokenAccount0: accounts.token0Account,
      tokenAccount1: accounts.token1Account,
      tokenVault0: poolState.tokenVault0,
      tokenVault1: poolState.tokenVault1,
      personalPosition,
      tokenProgram: TOKEN_PROGRAM_ID,
    }
  );
}

export async function decreaseLiquidity(
  accounts: LiquidityChangeAccounts,
  ammPool: AmmPool,
  positionState: PositionState,
  liquidity: BN,
  token0AmountSlippage?: number,
  token1AmountSlippage?: number
): Promise<TransactionInstruction> {
  const poolState = ammPool.poolState;
  const ctx = ammPool.ctx;
  const tickLowerIndex = positionState.tickLowerIndex;
  const tickUpperIndex = positionState.tickUpperIndex;
  const sqrtPriceLowerX64 =
    SqrtPriceMath.getSqrtPriceX64FromTick(tickLowerIndex);
  const sqrtPriceUpperX64 =
    SqrtPriceMath.getSqrtPriceX64FromTick(tickUpperIndex);

  const token0Amount = LiquidityMath.getToken0AmountForLiquidity(
    sqrtPriceLowerX64,
    sqrtPriceUpperX64,
    liquidity,
    false
  );
  const token1Amount = LiquidityMath.getToken1AmountForLiquidity(
    sqrtPriceLowerX64,
    sqrtPriceUpperX64,
    liquidity,
    false
  );
  let amount0Min: BN = new BN(0);
  let amount1Min: BN = new BN(0);
  if (token0AmountSlippage !== undefined) {
    amount0Min = token0Amount.muln(1 - token0AmountSlippage);
  }
  if (token1AmountSlippage !== undefined) {
    amount1Min = token1Amount.muln(1 - token1AmountSlippage);
  }
  // prepare tickArray
  const tickArrayLowerStartIndex = getArrayStartIndex(
    tickLowerIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayLower] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayLowerStartIndex
  );
  const tickArrayUpperStartIndex = getArrayStartIndex(
    tickUpperIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayUpper] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayUpperStartIndex
  );

  const positionANftAccount = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    positionState.nftMint,
    accounts.positionNftOwner
  );

  const [personalPosition] = await getPersonalPositionAddress(
    positionState.nftMint,
    ctx.program.programId
  );

  const [protocolPosition] = await getProtocolPositionAddress(
    ammPool.address,
    ctx.program.programId,
    tickLowerIndex,
    tickUpperIndex
  );

  return await decreaseLiquidityInstruction(
    ctx.program,
    {
      liquidity: liquidity,
      amount0Min,
      amount1Min,
    },
    {
      nftOwner: accounts.positionNftOwner,
      ammConfig: poolState.ammConfig,
      nftAccount: positionANftAccount,
      poolState: ammPool.address,
      protocolPosition,
      tickArrayLower,
      tickArrayUpper,
      recipientTokenAccount0: accounts.token0Account,
      recipientTokenAccount1: accounts.token1Account,
      tokenVault0: poolState.tokenVault0,
      tokenVault1: poolState.tokenVault1,
      personalPosition,
      tokenProgram: TOKEN_PROGRAM_ID,
    }
  );
}

export async function swapBaseIn(
  accounts: SwapAccounts,
  ammPool: AmmPool,
  inputTokenMint: PublicKey,
  amountIn: BN,
  amountOutSlippage?: number,
  sqrtPriceLimitX64?: BN
): Promise<TransactionInstruction> {
  const [expectedAmountOut, remainingAccounts] =
    await ammPool.getOutputAmountAndRemainAccounts(
      inputTokenMint,
      amountIn,
      sqrtPriceLimitX64,
      true
    );

  let amountOutMin = new BN(0);
  if (amountOutSlippage != undefined) {
    amountOutMin = expectedAmountOut.muln(1 - amountOutSlippage);
  }
  return swap(
    accounts,
    remainingAccounts,
    ammPool,
    inputTokenMint,
    amountIn,
    amountOutMin,
    true,
    sqrtPriceLimitX64
  );
}

export async function swapBaseOut(
  accounts: SwapAccounts,
  ammPool: AmmPool,
  outputTokenMint: PublicKey,
  amountOut: BN,
  amountInSlippage?: number,
  sqrtPriceLimitX64?: BN
): Promise<TransactionInstruction> {
  const [expectedAmountIn, remainingAccounts] =
    await ammPool.getInputAmountAndAccounts(
      outputTokenMint,
      amountOut,
      sqrtPriceLimitX64,
      true
    );
  let amountInMax = new BN(1).shln(32);
  if (amountInSlippage != undefined) {
    amountInMax = expectedAmountIn.muln(1 + amountInSlippage);
  }
  return swap(
    accounts,
    remainingAccounts,
    ammPool,
    outputTokenMint,
    amountOut,
    amountInMax,
    false,
    sqrtPriceLimitX64
  );
}

async function swap(
  accounts: SwapAccounts,
  remainingAccounts: AccountMeta[],
  ammPool: AmmPool,
  inputTokenMint: PublicKey,
  amount: BN,
  otherAmountThreshold: BN,
  isBaseInput: boolean,
  sqrtPriceLimitX64?: BN
): Promise<TransactionInstruction> {
  const poolState = ammPool.poolState;
  const ctx = ammPool.ctx;

  const tickArrayStartIndex = getArrayStartIndex(
    ammPool.poolState.tickCurrent,
    ammPool.poolState.tickSpacing
  );
  const [tickArray] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayStartIndex
  );

  // get vault
  const zeroForOne = isBaseInput
    ? inputTokenMint.equals(poolState.tokenMint0)
    : inputTokenMint.equals(poolState.tokenMint1);

  let inputVault: PublicKey = poolState.tokenVault0;
  let outputVault: PublicKey = poolState.tokenVault1;
  if (!zeroForOne) {
    inputVault = poolState.tokenVault1;
    outputVault = poolState.tokenVault0;
  }
  return await swapInstruction(
    ctx.program,
    {
      amount,
      otherAmountThreshold,
      sqrtPriceLimitX64,
      isBaseInput,
    },
    {
      payer: accounts.payer,
      ammConfig: poolState.ammConfig,
      poolState: ammPool.address,
      inputTokenAccount: accounts.inputTokenAccount,
      outputTokenAccount: accounts.outputTokenAccount,
      inputVault,
      outputVault,
      tickArray,
      observationState: ammPool.poolState.observationKey,
      remainings: [...remainingAccounts],
      tokenProgram: TOKEN_PROGRAM_ID,
    }
  );
}

export type RouterPoolParam = {
  ammPool: AmmPool;
  inputTokenMint: PublicKey;
  inputTokenAccount: PublicKey;
  outputTokenAccount: PublicKey;
};

type PrepareOnePoolResult = {
  amountOut: BN;
  outputTokenMint: PublicKey;
  outputTokenAccount: PublicKey;
  remains: AccountMeta[];
  additionLength: number;
};

async function prepareOnePool(
  inputAmount: BN,
  param: RouterPoolParam
): Promise<PrepareOnePoolResult> {
  // get vault
  const zeroForOne = param.inputTokenMint.equals(
    param.ammPool.poolState.tokenMint0
  );
  let inputVault: PublicKey = param.ammPool.poolState.tokenVault0;
  let outputVault: PublicKey = param.ammPool.poolState.tokenVault1;
  let outputTokenMint: PublicKey = param.ammPool.poolState.tokenMint1;
  if (!zeroForOne) {
    inputVault = param.ammPool.poolState.tokenVault1;
    outputVault = param.ammPool.poolState.tokenVault0;
    outputTokenMint = param.ammPool.poolState.tokenMint0;
  }
  const [expectedAmountOut, remainingAccounts] =
    await param.ammPool.getOutputAmountAndRemainAccounts(
      param.inputTokenMint,
      inputAmount
    );
  if (remainingAccounts.length == 0) {
    throw new Error("must has one tickArray");
  }
  return {
    amountOut: expectedAmountOut,
    outputTokenMint,
    outputTokenAccount: param.outputTokenAccount,
    remains: [
      {
        pubkey: param.ammPool.poolState.ammConfig,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: param.ammPool.address,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: param.outputTokenAccount,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: inputVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: outputVault,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: param.ammPool.poolState.observationKey,
        isSigner: false,
        isWritable: true,
      },
      ...remainingAccounts,
    ],
    additionLength: remainingAccounts.length-1,
  };
}

export async function swapRouterBaseIn(
  payer: PublicKey,
  firstPoolParam: RouterPoolParam,
  remainRouterPools: {
    ammPool: AmmPool;
    outputTokenAccount: PublicKey;
  }[],
  amountIn: BN,
  amountOutSlippage?: number
): Promise<TransactionInstruction> {
  let additionalAccountsArray: number[] = [];
  let remainingAccounts: AccountMeta[] = [];

  let result = await prepareOnePool(amountIn, firstPoolParam);
  additionalAccountsArray.push(result.additionLength);
  remainingAccounts.push(...result.remains);
  for (let i = 0; i < remainRouterPools.length; i++) {
    const param: RouterPoolParam = {
      ammPool: remainRouterPools[i].ammPool,
      inputTokenMint: result.outputTokenMint,
      inputTokenAccount: result.outputTokenAccount,
      outputTokenAccount: remainRouterPools[i].outputTokenAccount,
    };
    result = await prepareOnePool(result.amountOut, param);
    additionalAccountsArray.push(result.additionLength);
    remainingAccounts.push(...result.remains);
  }
  let amountOutMin = new BN(0);
  if (amountOutSlippage != undefined) {
    amountOutMin = amountOutMin.muln(1 - amountOutSlippage);
  }
  return await swapRouterBaseInInstruction(
    firstPoolParam.ammPool.ctx.program,
    {
      amountIn,
      amountOutMinimum: amountOutMin,
      additionalAccountsPerPool: Buffer.from(additionalAccountsArray),
    },
    {
      payer,
      inputTokenAccount: firstPoolParam.inputTokenAccount,
      tokenProgram: TOKEN_PROGRAM_ID,
      remainings: remainingAccounts,
    }
  );
}

export async function collectFee(
  accounts: LiquidityChangeAccounts,
  ammPool: AmmPool,
  positionState: PositionState,
  amount0Max: BN,
  amount1Max: BN
): Promise<TransactionInstruction> {
  const poolState = ammPool.poolState;
  const ctx = ammPool.ctx;
  const tickLowerIndex = positionState.tickLowerIndex;
  const tickUpperIndex = positionState.tickUpperIndex;

  // prepare tickArray
  const tickArrayLowerStartIndex = getArrayStartIndex(
    tickLowerIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayLower] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayLowerStartIndex
  );
  const tickArrayUpperStartIndex = getArrayStartIndex(
    tickUpperIndex,
    ammPool.poolState.tickSpacing
  );
  const [tickArrayUpper] = await getTickArrayAddress(
    ammPool.address,
    ctx.program.programId,
    tickArrayUpperStartIndex
  );

  const positionANftAccount = await Token.getAssociatedTokenAddress(
    ASSOCIATED_TOKEN_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
    positionState.nftMint,
    accounts.positionNftOwner
  );

  const [personalPosition] = await getPersonalPositionAddress(
    positionState.nftMint,
    ctx.program.programId
  );

  const [protocolPosition] = await getProtocolPositionAddress(
    ammPool.address,
    ctx.program.programId,
    tickLowerIndex,
    tickUpperIndex
  );

  return await collectFeeInstruction(
    ctx.program,
    {
      amount0Max,
      amount1Max,
    },
    {
      nftOwner: accounts.positionNftOwner,
      ammConfig: poolState.ammConfig,
      nftAccount: positionANftAccount,
      poolState: ammPool.address,
      protocolPosition,
      tickArrayLower,
      tickArrayUpper,
      recipientTokenAccount0: accounts.token0Account,
      recipientTokenAccount1: accounts.token1Account,
      tokenVault0: poolState.tokenVault0,
      tokenVault1: poolState.tokenVault1,
      personalPosition,
      tokenProgram: TOKEN_PROGRAM_ID,
    }
  );
}
