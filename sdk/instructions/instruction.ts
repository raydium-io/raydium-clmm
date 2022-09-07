import { BN } from "@project-serum/anchor";
import {
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  TransactionInstruction,
  AccountMeta,
  Signer,
} from "@solana/web3.js";

import { programs } from "@metaplex/js";
import { getTickArrayStartIndexByTick } from "../entities";
import {
  SqrtPriceMath,
  LiquidityMath,
  ONE,
  MIN_SQRT_PRICE_X64,
  MAX_SQRT_PRICE_X64,
  MathUtil,
} from "../math";

import { PersonalPositionState } from "../states";

import {
  getAmmConfigAddress,
  getPoolAddress,
  getPoolVaultAddress,
  getProtocolPositionAddress,
  getNftMetadataAddress,
  getPersonalPositionAddress,
  getTickArrayAddress,
  isWSOLTokenMint,
  makeCreateWrappedNativeAccountInstructions,
  makeCloseAccountInstruction,
  getPoolRewardVaultAddress,
} from "../utils";

import {
  Token,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  NATIVE_MINT,
} from "@solana/spl-token";

import {
  openPositionInstruction,
  closePositionInstruction,
  createPoolInstruction,
  increaseLiquidityInstruction,
  decreaseLiquidityInstruction,
  swapInstruction,
  swapRouterBaseInInstruction,
} from "./user";
import {
  createAmmConfigInstruction,
  updateAmmConfigInstruction,
  initializeRewardInstruction,
  setRewardParamsInstruction,
} from "./admin";

import { AmmPool } from "../pool";
import { Context } from "../base";
import Decimal from "decimal.js";

type CreatePoolAccounts = {
  poolCreator: PublicKey;
  ammConfig: PublicKey;
  tokenMint0: PublicKey;
  tokenMint1: PublicKey;
  observation: PublicKey;
};

export type OpenPositionAccounts = {
  payer: PublicKey;
  positionNftOwner: PublicKey;
  positionNftMint: PublicKey;
};

export type ClosePositionAccounts = {
  nftOwner: PublicKey;
  nftAccount: PublicKey;
  positionNftMint: PublicKey;
  personalPosition: PublicKey;
  tokenProgram: PublicKey;
  systemProgram: PublicKey;
};

export type LiquidityChangeAccounts = {
  positionNftOwner: PublicKey;
};

export type SwapAccounts = {
  payer: PublicKey;
};

export type RouterPoolParam = {
  ammPool: AmmPool;
  inputTokenMint: PublicKey;
};

type PrepareOnePoolResult = {
  amountOut: BN;
  inputTokenAccount: PublicKey;
  outputTokenMint: PublicKey;
  outputTokenAccount: PublicKey;
  remains: AccountMeta[];
};

export class AmmInstruction {
  private constructor() {}

  /**
   *
   * @param ctx
   * @param authority
   * @param index
   * @param tickSpacing
   * @param tradeFeeRate
   * @param protocolFeeRate
   * @returns
   */
  public static async createAmmConfig(
    ctx: Context,
    authority: PublicKey,
    index: number,
    tickSpacing: number,
    tradeFeeRate: number,
    protocolFeeRate: number
  ): Promise<[PublicKey, TransactionInstruction]> {
    const [address, _] = await getAmmConfigAddress(
      index,
      ctx.program.programId
    );
    return [
      address,
      await createAmmConfigInstruction(
        ctx.program,
        {
          index,
          tickSpacing,
          tradeFeeRate: tradeFeeRate,
          protocolFeeRate,
        },
        {
          owner: authority,
          ammConfig: address,
          systemProgram: SystemProgram.programId,
        }
      ),
    ];
  }

  public static async setAmmConfigNewOwner(
    ctx: Context,
    ammConfig: PublicKey,
    authority: PublicKey,
    newOwner: PublicKey
  ): Promise<TransactionInstruction> {
    return await updateAmmConfigInstruction(
      ctx.program,
      {
        newOwner,
        tradeFeeRate: 0,
        protocolFeeRate: 0,
        flag: 0,
      },
      {
        owner: authority,
        ammConfig,
      }
    );
  }

  public static async setAmmConfigTradeFeeRate(
    ctx: Context,
    ammConfig: PublicKey,
    authority: PublicKey,
    tradeFeeRate: number
  ): Promise<TransactionInstruction> {
    return await updateAmmConfigInstruction(
      ctx.program,
      {
        newOwner: PublicKey.default,
        tradeFeeRate: tradeFeeRate,
        protocolFeeRate: 0,
        flag: 1,
      },
      {
        owner: authority,
        ammConfig,
      }
    );
  }

  public static async setAmmConfigProtocolFeeRate(
    ctx: Context,
    ammConfig: PublicKey,
    authority: PublicKey,
    protocolFeeRate: number
  ): Promise<TransactionInstruction> {
    return await updateAmmConfigInstruction(
      ctx.program,
      {
        newOwner: PublicKey.default,
        tradeFeeRate: 0,
        protocolFeeRate: protocolFeeRate,
        flag: 2,
      },
      {
        owner: authority,
        ammConfig,
      }
    );
  }

  public static async initializeReward(
    ctx: Context,
    authority: PublicKey,
    ammPool: AmmPool,
    rewardTokenMint: PublicKey,
    rewardIndex: number,
    openTime: BN,
    endTime: BN,
    emissionsPerSecond: number
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount, isWSol } = await getATAOrRandomWsolTokenAccount(
      ctx,
      authority,
      rewardTokenMint,
      new BN(0),
      instructions,
      signers
    );

    const [rewardTokenVault] = await getPoolRewardVaultAddress(
      ammPool.address,
      rewardTokenMint,
      ctx.program.programId
    );

    const emissionsPerSecondX64 = MathUtil.decimalToX64(
      new Decimal(emissionsPerSecond)
    );
    const ix = await initializeRewardInstruction(
      ctx.program,
      {
        rewardIndex,
        openTime,
        endTime,
        emissionsPerSecondX64,
      },
      {
        rewardFunder: authority,
        rewardTokenVault: rewardTokenVault,
        funderTokenAccount: tokenAccount,
        rewardTokenMint: rewardTokenMint,
        ammConfig: ammPool.poolState.ammConfig,
        poolState: ammPool.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      }
    );

    instructions.push(ix);

    if (isWSol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount,
        owner: authority,
        payer: authority,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }

  public static async setRewardParams(
    ctx: Context,
    authority: PublicKey,
    ammPool: AmmPool,
    rewardIndex: number,
    emissionsPerSecond: number,
    openTimestamp: BN,
    endTimestamp: BN
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const rewardInfo = ammPool.poolState.rewardInfos[rewardIndex];
    const remainAccouts: AccountMeta[] = [];
    const [rewardTokenVault] = await getPoolRewardVaultAddress(
      ammPool.address,
      rewardInfo.tokenMint,
      ctx.program.programId
    );
    remainAccouts.push({
      pubkey: rewardTokenVault,
      isSigner: false,
      isWritable: true,
    });
    const { tokenAccount, isWSol } = await getATAOrRandomWsolTokenAccount(
      ctx,
      authority,
      rewardInfo.tokenMint,
      new BN(0),
      instructions,
      signers
    );
    remainAccouts.push({
      pubkey: tokenAccount,
      isSigner: false,
      isWritable: true,
    });

    remainAccouts.push({
      pubkey: TOKEN_PROGRAM_ID,
      isSigner: false,
      isWritable: false,
    });

    const emissionsPerSecondX64 = MathUtil.decimalToX64(
      new Decimal(emissionsPerSecond)
    );

    const ix = await setRewardParamsInstruction(
      ctx.program,
      {
        rewardIndex,
        emissionsPerSecondX64,
        openTimestamp,
        endTimestamp,
      },
      {
        authority,
        ammConfig: ammPool.poolState.ammConfig,
        poolState: ammPool.address,
      },
      remainAccouts
    );
    instructions.push(ix);

    if (isWSol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount,
        owner: authority,
        payer: authority,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }
  /**
   *
   * @param ctx
   * @param accounts
   * @param initialPrice
   * @returns
   */
  public static async createPool(
    ctx: Context,
    accounts: CreatePoolAccounts,
    initialPrice: Decimal,
    tokenMint0Decimals: number,
    tokenMint1Decimals: number
  ): Promise<[PublicKey, TransactionInstruction]> {
    // @ts-ignore
    if ((accounts.tokenMint0._bn as BN).gt(accounts.tokenMint1._bn as BN)) {
      const tmp = accounts.tokenMint0;
      accounts.tokenMint0 = accounts.tokenMint1;
      accounts.tokenMint1 = tmp;
      initialPrice = initialPrice.dividedBy(1);
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

    const initialPriceX64 = SqrtPriceMath.priceToSqrtPriceX64(
      initialPrice,
      tokenMint0Decimals,
      tokenMint1Decimals
    );
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

  /**
   *
   * @param accounts
   * @param ammPool
   * @param priceLower
   * @param priceUpper
   * @param liquidity
   * @param amountSlippage
   * @returns
   */
  public static async openPositionWithPrice(
    accounts: OpenPositionAccounts,
    ammPool: AmmPool,
    priceLower: Decimal,
    priceUpper: Decimal,
    tokenMint0Decimals: number,
    tokenMint1Decimals: number,
    liquidity: BN,
    amountSlippage?: number
  ): Promise<{
    personalPosition: PublicKey;
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    const tickLower = SqrtPriceMath.getTickFromPrice(
      priceLower,
      tokenMint0Decimals,
      tokenMint1Decimals
    );
    const tickUpper = SqrtPriceMath.getTickFromPrice(
      priceUpper,
      tokenMint0Decimals,
      tokenMint1Decimals
    );

    return AmmInstruction.openPosition(
      accounts,
      ammPool,
      tickLower,
      tickUpper,
      liquidity,
      amountSlippage
    );
  }

  /**
   *
   * @param accounts
   * @param ammPool
   * @param tickLowerIndex
   * @param tickUpperIndex
   * @param liquidity
   * @param amountSlippage
   * @returns
   */
  public static async openPosition(
    accounts: OpenPositionAccounts,
    ammPool: AmmPool,
    tickLowerIndex: number,
    tickUpperIndex: number,
    liquidity: BN,
    amountSlippage?: number
  ): Promise<{
    personalPosition: PublicKey;
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountSlippage != undefined && amountSlippage < 0) {
      throw new Error("amountSlippage must be gtn 0");
    }
    if (tickLowerIndex % ammPool.poolState.tickSpacing != 0) {
      throw new Error(
        "tickLowIndex must be an integer multiple of tickspacing"
      );
    }
    if (tickUpperIndex % ammPool.poolState.tickSpacing != 0) {
      throw new Error(
        "tickUpperIndex must be an integer multiple of tickspacing"
      );
    }

    const poolState = ammPool.poolState;
    const ctx = ammPool.ctx;
    const [amount0Max, amount1Max] =
      LiquidityMath.getAmountsFromLiquidityWithSlippage(
        poolState.sqrtPriceX64,
        SqrtPriceMath.getSqrtPriceX64FromTick(tickLowerIndex),
        SqrtPriceMath.getSqrtPriceX64FromTick(tickUpperIndex),
        liquidity,
        true,
        true,
        amountSlippage
      );
    // prepare tickArray
    const tickArrayLowerStartIndex = getTickArrayStartIndexByTick(
      tickLowerIndex,
      ammPool.poolState.tickSpacing
    );
    const [tickArrayLower] = await getTickArrayAddress(
      ammPool.address,
      ctx.program.programId,
      tickArrayLowerStartIndex
    );
    const tickArrayUpperStartIndex = getTickArrayStartIndexByTick(
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

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount: token0Account, isWSol: isToken0WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.payer,
        poolState.tokenMint0,
        amount0Max,
        instructions,
        signers
      );

    const { tokenAccount: token1Account, isWSol: isToken1WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.payer,
        poolState.tokenMint1,
        amount1Max,
        instructions,
        signers
      );

    const openIx = await openPositionInstruction(
      ctx.program,
      {
        tickLowerIndex,
        tickUpperIndex,
        tickArrayLowerStartIndex: tickArrayLowerStartIndex,
        tickArrayUpperStartIndex: tickArrayUpperStartIndex,
        liquidity: liquidity,
        amount0Max: amount0Max,
        amount1Max: amount1Max,
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
        tokenAccount0: token0Account,
        tokenAccount1: token1Account,
        tokenVault0: poolState.tokenVault0,
        tokenVault1: poolState.tokenVault1,
        personalPosition,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: programs.metadata.MetadataProgram.PUBKEY,
      }
    );
    instructions.push(openIx);

    if (isToken0WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token0Account,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }
    if (isToken1WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token1Account,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }

    return {
      personalPosition,
      instructions,
      signers,
    };
  }

  public static async closePosition(
    accounts: ClosePositionAccounts,
    ammPool: AmmPool
  ): Promise<TransactionInstruction> {
    return closePositionInstruction(ammPool.ctx.program, accounts);
  }

  /**
   *
   * @param accounts
   * @param ammPool
   * @param positionState
   * @param liquidity
   * @param amountSlippage
   * @returns
   */
  public static async increaseLiquidity(
    accounts: LiquidityChangeAccounts,
    ammPool: AmmPool,
    positionState: PersonalPositionState,
    liquidity: BN,
    amountSlippage?: number
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountSlippage != undefined && amountSlippage < 0) {
      throw new Error("amountSlippage must be gtn 0");
    }
    const poolState = ammPool.poolState;
    const ctx = ammPool.ctx;
    const tickLowerIndex = positionState.tickLowerIndex;
    const tickUpperIndex = positionState.tickUpperIndex;

    const [amount0Max, amount1Max] =
      LiquidityMath.getAmountsFromLiquidityWithSlippage(
        poolState.sqrtPriceX64,
        SqrtPriceMath.getSqrtPriceX64FromTick(tickLowerIndex),
        SqrtPriceMath.getSqrtPriceX64FromTick(tickUpperIndex),
        liquidity,
        true,
        true,
        amountSlippage
      );
    // console.log(
    //   "increaseLiquidity amount0Max:",
    //   amount0Max.toString(),
    //   "amount1Max:",
    //   amount1Max.toString()
    // );
    // prepare tickArray
    const tickArrayLowerStartIndex = getTickArrayStartIndexByTick(
      tickLowerIndex,
      ammPool.poolState.tickSpacing
    );
    const [tickArrayLower] = await getTickArrayAddress(
      ammPool.address,
      ctx.program.programId,
      tickArrayLowerStartIndex
    );
    const tickArrayUpperStartIndex = getTickArrayStartIndexByTick(
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

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount: token0Account, isWSol: isToken0WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.positionNftOwner,
        poolState.tokenMint0,
        amount0Max,
        instructions,
        signers
      );

    const { tokenAccount: token1Account, isWSol: isToken1WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.positionNftOwner,
        poolState.tokenMint1,
        amount1Max,
        instructions,
        signers
      );

    const ix = await increaseLiquidityInstruction(
      ctx.program,
      {
        liquidity,
        amount0Max,
        amount1Max,
      },
      {
        nftOwner: accounts.positionNftOwner,
        nftAccount: positionANftAccount,
        poolState: ammPool.address,
        protocolPosition,
        tickArrayLower,
        tickArrayUpper,
        tokenAccount0: token0Account,
        tokenAccount1: token1Account,
        tokenVault0: poolState.tokenVault0,
        tokenVault1: poolState.tokenVault1,
        personalPosition,
        tokenProgram: TOKEN_PROGRAM_ID,
      }
    );
    instructions.push(ix);

    if (isToken0WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token0Account,
        owner: accounts.positionNftOwner,
        payer: accounts.positionNftOwner,
      });
      instructions.push(closeIx);
    }
    if (isToken1WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token1Account,
        owner: accounts.positionNftOwner,
        payer: accounts.positionNftOwner,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }

  /**
   *  decrease liquidity, collect fee and rewards
   * @param accounts
   * @param ammPool
   * @param positionState
   * @param token0AmountDesired
   * @param token1AmountDesired
   * @param amountSlippage
   * @returns
   */
  public static async decreaseLiquidityWithInputAmount(
    accounts: LiquidityChangeAccounts,
    ammPool: AmmPool,
    positionState: PersonalPositionState,
    token0AmountDesired: BN,
    token1AmountDesired: BN,
    amountSlippage?: number
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountSlippage != undefined && amountSlippage < 0) {
      throw new Error("amountSlippage must be gtn 0");
    }
    const price_lower = SqrtPriceMath.getSqrtPriceX64FromTick(
      positionState.tickLowerIndex
    );
    const price_upper = SqrtPriceMath.getSqrtPriceX64FromTick(
      positionState.tickUpperIndex
    );
    const liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
      ammPool.poolState.sqrtPriceX64,
      price_lower,
      price_upper,
      token0AmountDesired,
      token1AmountDesired
    );
    return AmmInstruction.decreaseLiquidity(
      accounts,
      ammPool,
      positionState,
      liquidity,
      amountSlippage
    );
  }

  /**
   * decrease liquidity, collect fee and rewards
   * @param accounts
   * @param ammPool
   * @param positionState
   * @param liquidity
   * @param amountSlippage
   * @returns
   */
  public static async decreaseLiquidity(
    accounts: LiquidityChangeAccounts,
    ammPool: AmmPool,
    positionState: PersonalPositionState,
    liquidity: BN,
    amountSlippage?: number
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountSlippage != undefined && amountSlippage < 0) {
      throw new Error("amountSlippage must be gtn 0");
    }
    const ctx = ammPool.ctx;
    const tickLowerIndex = positionState.tickLowerIndex;
    const tickUpperIndex = positionState.tickUpperIndex;
    const sqrtPriceLowerX64 =
      SqrtPriceMath.getSqrtPriceX64FromTick(tickLowerIndex);
    const sqrtPriceUpperX64 =
      SqrtPriceMath.getSqrtPriceX64FromTick(tickUpperIndex);

    const [amount0Min, amount1Min] =
      LiquidityMath.getAmountsFromLiquidityWithSlippage(
        ammPool.poolState.sqrtPriceX64,
        sqrtPriceLowerX64,
        sqrtPriceUpperX64,
        liquidity,
        false,
        false,
        amountSlippage
      );
    // prepare tickArray
    const tickArrayLowerStartIndex = getTickArrayStartIndexByTick(
      tickLowerIndex,
      ammPool.poolState.tickSpacing
    );
    const [tickArrayLower] = await getTickArrayAddress(
      ammPool.address,
      ctx.program.programId,
      tickArrayLowerStartIndex
    );
    const tickArrayUpperStartIndex = getTickArrayStartIndexByTick(
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

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount: token0Account, isWSol: isToken0WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.positionNftOwner,
        ammPool.poolState.tokenMint0,
        new BN(0),
        instructions,
        signers
      );

    const { tokenAccount: token1Account, isWSol: isToken1WsolAccount } =
      await getATAOrRandomWsolTokenAccount(
        ctx,
        accounts.positionNftOwner,
        ammPool.poolState.tokenMint1,
        new BN(0),
        instructions,
        signers
      );

    const { rewardAccounts, wSolAccount } = await getRewardAccounts(
      ctx,
      accounts.positionNftOwner,
      ammPool,
      instructions,
      signers
    );

    const ix = await decreaseLiquidityInstruction(
      ctx.program,
      {
        liquidity,
        amount0Min,
        amount1Min,
      },
      {
        nftOwner: accounts.positionNftOwner,
        nftAccount: positionANftAccount,
        poolState: ammPool.address,
        protocolPosition,
        tickArrayLower,
        tickArrayUpper,
        recipientTokenAccount0: token0Account,
        recipientTokenAccount1: token1Account,
        tokenVault0: ammPool.poolState.tokenVault0,
        tokenVault1: ammPool.poolState.tokenVault1,
        personalPosition,
        tokenProgram: TOKEN_PROGRAM_ID,
      },
      rewardAccounts
    );
    instructions.push(ix);

    if (isToken0WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token0Account,
        owner: accounts.positionNftOwner,
        payer: accounts.positionNftOwner,
      });
      instructions.push(closeIx);
    }
    if (isToken1WsolAccount) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: token1Account,
        owner: accounts.positionNftOwner,
        payer: accounts.positionNftOwner,
      });
      instructions.push(closeIx);
    }
    if (!wSolAccount.equals(PublicKey.default)) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: wSolAccount,
        owner: accounts.positionNftOwner,
        payer: accounts.positionNftOwner,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }

  /**
   *
   * @param accounts
   * @param ammPool
   * @param inputTokenMint
   * @param amountIn
   * @param amountOutSlippage
   * @param priceLimit
   * @returns
   */
  public static async swapBaseIn(
    accounts: SwapAccounts,
    ammPool: AmmPool,
    inputTokenMint: PublicKey,
    amountIn: BN,
    amountOutSlippage?: number,
    priceLimit?: Decimal
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountOutSlippage != undefined && amountOutSlippage < 0) {
      throw new Error("amountOutSlippage must be gtn 0");
    }
    let sqrtPriceLimitX64 = new BN(0);
    const zeroForOne = inputTokenMint.equals(ammPool.poolState.tokenMint0);
    if (priceLimit == undefined || priceLimit.eq(new Decimal(0))) {
      sqrtPriceLimitX64 = zeroForOne
        ? MIN_SQRT_PRICE_X64.add(ONE)
        : MAX_SQRT_PRICE_X64.sub(ONE);
    } else {
      sqrtPriceLimitX64 = SqrtPriceMath.priceToSqrtPriceX64(
        priceLimit,
        ammPool.poolState.mintDecimals0,
        ammPool.poolState.mintDecimals1
      );
    }
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
    console.log(
      "swapBaseIn amountIn:",
      amountIn.toString(),
      "expectedAmountOut:",
      expectedAmountOut.toString(),
      "amountOutMin:",
      amountOutMin.toString()
    );
    let outputTokenMint = PublicKey.default;
    if (zeroForOne) {
      outputTokenMint = ammPool.poolState.tokenMint1;
    } else {
      outputTokenMint = ammPool.poolState.tokenMint0;
    }

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount: inputTokenAccount, isWSol: isInputTokenWsol } =
      await getATAOrRandomWsolTokenAccount(
        ammPool.ctx,
        accounts.payer,
        inputTokenMint,
        amountIn,
        instructions,
        signers
      );

    const { tokenAccount: outputTokenAccount, isWSol: isOutputTokenWsol } =
      await getATAOrRandomWsolTokenAccount(
        ammPool.ctx,
        accounts.payer,
        outputTokenMint,
        new BN(0),
        instructions,
        signers
      );

    const ix = await AmmInstruction.swap(
      accounts.payer,
      inputTokenAccount,
      outputTokenAccount,
      remainingAccounts,
      ammPool,
      inputTokenMint,
      amountIn,
      amountOutMin,
      true,
      sqrtPriceLimitX64
    );
    instructions.push(ix);

    if (isInputTokenWsol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: inputTokenAccount,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }
    if (isOutputTokenWsol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: outputTokenAccount,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }

  /**
   *
   * @param accounts
   * @param ammPool
   * @param outputTokenMint
   * @param amountOut
   * @param amountInSlippage
   * @param priceLimit
   * @returns
   */
  public static async swapBaseOut(
    accounts: SwapAccounts,
    ammPool: AmmPool,
    outputTokenMint: PublicKey,
    amountOut: BN,
    amountInSlippage?: number,
    priceLimit?: Decimal
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountInSlippage != undefined && amountInSlippage < 0) {
      throw new Error("amountInSlippage must be gtn 0");
    }
    let sqrtPriceLimitX64 = new BN(0);
    const zeroForOne = outputTokenMint.equals(ammPool.poolState.tokenMint1);
    if (priceLimit == undefined || priceLimit.eq(new Decimal(0))) {
      sqrtPriceLimitX64 = zeroForOne
        ? MIN_SQRT_PRICE_X64.add(ONE)
        : MAX_SQRT_PRICE_X64.sub(ONE);
    } else {
      sqrtPriceLimitX64 = SqrtPriceMath.priceToSqrtPriceX64(
        priceLimit,
        ammPool.poolState.mintDecimals0,
        ammPool.poolState.mintDecimals1
      );
    }
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
    console.log(
      "swapBaseOut amountOut:",
      amountOut.toString(),
      "expectedAmountIn:",
      expectedAmountIn.toString(),
      "amountInMax:",
      amountInMax.toString()
    );
    let inputTokenMint = PublicKey.default;
    if (new PublicKey(outputTokenMint).equals(ammPool.poolState.tokenMint0)) {
      inputTokenMint = ammPool.poolState.tokenMint1;
    } else {
      inputTokenMint = ammPool.poolState.tokenMint0;
    }

    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const { tokenAccount: inputTokenAccount, isWSol: isInputTokenWsol } =
      await getATAOrRandomWsolTokenAccount(
        ammPool.ctx,
        accounts.payer,
        inputTokenMint,
        amountInMax,
        instructions,
        signers
      );

    const { tokenAccount: outputTokenAccount, isWSol: isOutputTokenWsol } =
      await getATAOrRandomWsolTokenAccount(
        ammPool.ctx,
        accounts.payer,
        outputTokenMint,
        new BN(0),
        instructions,
        signers
      );

    const ix = await AmmInstruction.swap(
      accounts.payer,
      inputTokenAccount,
      outputTokenAccount,
      remainingAccounts,
      ammPool,
      outputTokenMint,
      amountOut,
      amountInMax,
      false,
      sqrtPriceLimitX64
    );
    instructions.push(ix);

    if (isInputTokenWsol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: inputTokenAccount,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }
    if (isOutputTokenWsol) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: outputTokenAccount,
        owner: accounts.payer,
        payer: accounts.payer,
      });
      instructions.push(closeIx);
    }
    return {
      instructions,
      signers,
    };
  }

  /**
   *
   * @param payer
   * @param firstPoolParam
   * @param remainRouterPools
   * @param amountIn
   * @param amountOutSlippage
   * @returns
   */
  public static async swapRouterBaseIn(
    payer: PublicKey,
    firstPoolParam: RouterPoolParam,
    remainRouterPools: AmmPool[],
    amountIn: BN,
    amountOutSlippage?: number
  ): Promise<{
    instructions: TransactionInstruction[];
    signers: Signer[];
  }> {
    if (amountOutSlippage != undefined && amountOutSlippage < 0) {
      throw new Error("amountOutSlippage must be gtn 0");
    }
    let remainingAccounts: AccountMeta[] = [];
    let instructions: TransactionInstruction[] = [];
    let signers: Signer[] = [];

    const inputTokenMint = new PublicKey(firstPoolParam.inputTokenMint);
    let wSolAccount = PublicKey.default;
    let needWSolTokenAccount = false;
    let allPool: AmmPool[] = [firstPoolParam.ammPool, ...remainRouterPools];
    for (const pool of allPool) {
      if (
        isWSOLTokenMint(pool.poolState.tokenMint0) ||
        isWSOLTokenMint(pool.poolState.tokenMint1)
      ) {
        needWSolTokenAccount = true;
        break;
      }
    }
    if (needWSolTokenAccount) {
      if (isWSOLTokenMint(inputTokenMint)) {
        const { tokenAccount: inputTokenAccount } =
          await getATAOrRandomWsolTokenAccount(
            firstPoolParam.ammPool.ctx,
            payer,
            inputTokenMint,
            amountIn,
            instructions,
            signers
          );
        wSolAccount = inputTokenAccount;
      } else {
        const { tokenAccount: randomWSolAccount } =
          await getATAOrRandomWsolTokenAccount(
            firstPoolParam.ammPool.ctx,
            payer,
            NATIVE_MINT,
            new BN(0),
            instructions,
            signers
          );
        wSolAccount = randomWSolAccount;
      }
    }

    let result = await AmmInstruction.prepareOnePool(
      payer,
      amountIn,
      firstPoolParam,
      wSolAccount
    );
    remainingAccounts.push(...result.remains);
    const startInputTokenAccount = result.inputTokenAccount;
    for (let i = 0; i < remainRouterPools.length; i++) {
      const param: RouterPoolParam = {
        ammPool: remainRouterPools[i],
        inputTokenMint: result.outputTokenMint,
      };
      result = await AmmInstruction.prepareOnePool(
        payer,
        result.amountOut,
        param,
        wSolAccount
      );
      remainingAccounts.push(...result.remains);
    }
    let amountOutMin = new BN(0);
    if (amountOutSlippage != undefined) {
      amountOutMin = amountOutMin.muln(1 - amountOutSlippage);
    }
    const ix = await swapRouterBaseInInstruction(
      firstPoolParam.ammPool.ctx.program,
      {
        amountIn,
        amountOutMinimum: amountOutMin,
      },
      {
        payer,
        inputTokenAccount: startInputTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        remainings: remainingAccounts,
      }
    );
    instructions.push(ix);
    if (!wSolAccount.equals(PublicKey.default)) {
      const closeIx = makeCloseAccountInstruction({
        tokenAccount: wSolAccount,
        owner: payer,
        payer: payer,
      });
      instructions.push(closeIx);
    }

    return {
      instructions,
      signers,
    };
  }

  static async swap(
    payer: PublicKey,
    inputTokenAccount: PublicKey,
    outputTokenAccount: PublicKey,
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
    if (sqrtPriceLimitX64 == undefined) {
      sqrtPriceLimitX64 = new BN(0);
    }
    const tickArray = remainingAccounts[0].pubkey;
    if (remainingAccounts.length >= 1) {
      remainingAccounts = remainingAccounts.slice(1, remainingAccounts.length);
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
        payer,
        ammConfig: poolState.ammConfig,
        poolState: ammPool.address,
        inputTokenAccount,
        outputTokenAccount,
        inputVault,
        outputVault,
        observationState: ammPool.poolState.observationKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        tickArray,
        remainings: [...remainingAccounts],

      }
    );
  }

  static async prepareOnePool(
    owner: PublicKey,
    inputAmount: BN,
    param: RouterPoolParam,
    wSolAccount?: PublicKey
  ): Promise<PrepareOnePoolResult> {
    if (!param.ammPool.isContain(param.inputTokenMint)) {
      throw new Error(
        `pool ${param.ammPool.address.toString()} is not contain token mint ${param.inputTokenMint.toString()}`
      );
    }
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
    let inputTokenAccount = PublicKey.default;
    if (isWSOLTokenMint(param.inputTokenMint)) {
      if (wSolAccount == undefined || wSolAccount.equals(PublicKey.default)) {
        throw new Error("wSol token account must be specialed");
      }
      inputTokenAccount = wSolAccount;
    } else {
      inputTokenAccount = await Token.getAssociatedTokenAddress(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        param.inputTokenMint,
        owner
      );
    }
    let outputTokenAccount = PublicKey.default;
    if (isWSOLTokenMint(outputTokenMint)) {
      if (wSolAccount == undefined || wSolAccount.equals(PublicKey.default)) {
        throw new Error("wSol token account must be specialed");
      }
      outputTokenAccount = wSolAccount;
    } else {
      outputTokenAccount = await Token.getAssociatedTokenAddress(
        ASSOCIATED_TOKEN_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
        outputTokenMint,
        owner
      );
    }
    return {
      amountOut: expectedAmountOut,
      inputTokenAccount,
      outputTokenMint,
      outputTokenAccount,
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
          pubkey: outputTokenAccount,
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
    };
  }
}

async function getATAOrRandomWsolTokenAccount(
  ctx: Context,
  owner: PublicKey,
  tokenMint: PublicKey,
  amount: BN,
  instructions: TransactionInstruction[],
  signers: Signer[]
): Promise<{
  tokenAccount: PublicKey;
  isWSol: boolean;
}> {
  let isWSol = false;
  let tokenAccount = PublicKey.default;
  if (isWSOLTokenMint(tokenMint)) {
    const { newAccount, instructions: ixs } =
      await makeCreateWrappedNativeAccountInstructions({
        connection: ctx.connection,
        owner: owner,
        payer: owner,
        amount: amount,
      });
    isWSol = true;
    signers.push(newAccount);
    tokenAccount = newAccount.publicKey;
    instructions.push(...ixs);
    console.log("new wsol account:", newAccount.publicKey.toBase58());
  } else {
    tokenAccount = await Token.getAssociatedTokenAddress(
      ASSOCIATED_TOKEN_PROGRAM_ID,
      TOKEN_PROGRAM_ID,
      tokenMint,
      owner
    );
  }
  return {
    tokenAccount,
    isWSol,
  };
}

async function getRewardAccounts(
  ctx: Context,
  owner: PublicKey,
  pool: AmmPool,
  instructions: TransactionInstruction[],
  signers: Signer[]
): Promise<{
  rewardAccounts: AccountMeta[];
  wSolAccount: PublicKey;
}> {
  var rewardAccounts: AccountMeta[] = [];
  var wSolAccount = PublicKey.default;
  for (const rewardInfo of pool.poolState.rewardInfos) {
    if (!rewardInfo.tokenMint.equals(PublicKey.default)) {
      const [rewardTokenVault] = await getPoolRewardVaultAddress(
        pool.address,
        rewardInfo.tokenMint,
        ctx.program.programId
      );
      rewardAccounts.push({
        pubkey: rewardTokenVault,
        isSigner: false,
        isWritable: true,
      });

      const { tokenAccount: ownerRewardTokenAccount, isWSol } =
        await getATAOrRandomWsolTokenAccount(
          ctx,
          owner,
          rewardInfo.tokenMint,
          new BN(0),
          instructions,
          signers
        );
      rewardAccounts.push({
        pubkey: ownerRewardTokenAccount,
        isSigner: false,
        isWritable: true,
      });
      if (isWSol) {
        wSolAccount = ownerRewardTokenAccount;
      }
    }
  }
  return { rewardAccounts, wSolAccount };
}
