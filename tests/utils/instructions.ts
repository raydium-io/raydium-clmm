import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../../target/types/raydium_clmm";
import {
  PublicKey,
  Keypair,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  AccountMeta,
  ComputeBudgetProgram,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
} from "@solana/spl-token";
import {
  MEMO_PROGRAM_ID,
  METADATA_PROGRAM_ID,
} from "@raydium-io/raydium-sdk-v2";
import { getTickArrayAddressByTick } from "./util";
import { TickArrayUtil, TickUtil } from "@raydium-io/raydium-sdk-v2";
import { PDAUtils } from "./pda";

/**
 * Everything a caller needs to act on a position after opening it: the NFT it
 * is bound to, the PDAs later instructions require, and the token program the
 * NFT itself lives under (classic Token for openPosition/openPositionV2,
 * Token-2022 for openPositionWithToken22Nft).
 */
export interface OpenedPosition {
  signature: string;
  poolState: PublicKey;
  token0: PublicKey;
  token1: PublicKey;
  tickArrayLower: PublicKey;
  tickArrayUpper: PublicKey;
  protocolPosition: PublicKey;
  positionNftMint: PublicKey;
  positionNftAccount: PublicKey;
  personalPosition: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  nftTokenProgram: PublicKey;
}

/** Shared by all three open_position variants. */
export interface OpenPositionParams {
  payer: Keypair;
  poolState: PublicKey;
  tickLowerIndex: number;
  tickUpperIndex: number;
  liquidity: anchor.BN;
  amount0Max: anchor.BN;
  amount1Max: anchor.BN;
  positionNftOwner: PublicKey;
  tokenVault0Mint: PublicKey;
  tokenVault1Mint: PublicKey;
}

/**
 * Instruction helper for Raydium AMM V3
 * Contains helper functions for calling program instructions
 */
export class InstructionHelper {
  program: Program<RaydiumClmm>;
  provider: anchor.AnchorProvider;
  pda: PDAUtils;

  constructor(program: Program<RaydiumClmm>) {
    this.program = program;
    this.provider = anchor.AnchorProvider.env();
    anchor.setProvider(this.provider);
    this.pda = new PDAUtils(program.programId);
  }

  /**
   * Airdrop SOL helper
   */
  async airdrop(publicKey: PublicKey, amount: number) {
    const airdropSig = await this.provider.connection.requestAirdrop(
      publicKey,
      amount * anchor.web3.LAMPORTS_PER_SOL
    );
    await this.provider.connection.confirmTransaction(airdropSig);
    return airdropSig;
  }

  /**
   * Create AMN Config instruction
   */
  async createAmmConfig(params: {
    admin: Keypair;
    index: number;
    tickSpacing: number;
    tradeFeeRate: number;
    protocolFeeRate: number;
    fundFeeRate: number;
  }) {
    const [ammConfig] = await this.pda.getAmmConfigPDA(params.index);

    return await this.program.methods
      .createAmmConfig(
        params.index,
        params.tickSpacing,
        params.tradeFeeRate,
        params.protocolFeeRate,
        params.fundFeeRate
      )
      .accounts({
        admin: params.admin.publicKey,
        ammConfig: ammConfig,
        systemProgram: SystemProgram.programId,
      })
      .signers([params.admin])
      .rpc();
  }

  /**
   * Derives every address the three open_position variants share, so each
   * variant only has to spell out the accounts that actually differ.
   */
  private async derivePositionAddresses(
    params: OpenPositionParams,
    positionNftMint: PublicKey,
    nftTokenProgram: PublicKey
  ) {
    const poolStateData = await this.program.account.poolState.fetch(
      params.poolState
    );
    const positionNftAccount = getAssociatedTokenAddressSync(
      positionNftMint,
      params.positionNftOwner,
      false,
      nftTokenProgram
    );

    const [personalPosition] = await this.pda.getPersonalPositionStatePDA(
      positionNftMint
    );

    // Compute tick array start indexes from tickLowerIndex/tickUpperIndex
    const lowerStart = TickArrayUtil.getTickArrayStartIndex(
      params.tickLowerIndex,
      poolStateData.tickSpacing
    );
    const upperStart = TickArrayUtil.getTickArrayStartIndex(
      params.tickUpperIndex,
      poolStateData.tickSpacing
    );

    // Derive tick arrays
    const tickArrayLower = getTickArrayAddressByTick(
      this.program.programId,
      params.poolState,
      lowerStart,
      poolStateData.tickSpacing
    );

    const tickArrayUpper = getTickArrayAddressByTick(
      this.program.programId,
      params.poolState,
      upperStart,
      poolStateData.tickSpacing
    );
    // Derive protocol position
    const [protocolPosition] = await this.pda.getProtocolPositionStatePDA(
      params.poolState,
      params.tickLowerIndex,
      params.tickUpperIndex
    );

    const metadataAccount = await PublicKey.findProgramAddress(
      [
        Buffer.from("metadata"),
        METADATA_PROGRAM_ID.toBuffer(),
        positionNftMint.toBuffer(),
      ],
      METADATA_PROGRAM_ID
    ).then(([key]) => key);

    const tokenAccount0 = getAssociatedTokenAddressSync(
      params.tokenVault0Mint,
      params.payer.publicKey
    );
    const tokenAccount1 = getAssociatedTokenAddressSync(
      params.tokenVault1Mint,
      params.payer.publicKey
    );

    // Derive token vaults using PDA (pool_vault seed)
    const [tokenVault0] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.tokenVault0Mint
    );
    const [tokenVault1] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.tokenVault1Mint
    );

    return {
      positionNftAccount,
      personalPosition,
      lowerStart,
      upperStart,
      tickArrayLower,
      tickArrayUpper,
      protocolPosition,
      metadataAccount,
      tokenAccount0,
      tokenAccount1,
      tokenVault0,
      tokenVault1,
    };
  }

  /** Packs the derived addresses plus the tx signature into an OpenedPosition. */
  private toOpenedPosition(
    params: OpenPositionParams,
    addrs: Awaited<ReturnType<InstructionHelper["derivePositionAddresses"]>>,
    positionNftMint: PublicKey,
    nftTokenProgram: PublicKey,
    signature: string
  ): OpenedPosition {
    return {
      signature,
      poolState: params.poolState,
      token0: params.tokenVault0Mint,
      token1: params.tokenVault1Mint,
      tickArrayLower: addrs.tickArrayLower,
      tickArrayUpper: addrs.tickArrayUpper,
      protocolPosition: addrs.protocolPosition,
      positionNftMint,
      positionNftAccount: addrs.positionNftAccount,
      personalPosition: addrs.personalPosition,
      tokenVault0: addrs.tokenVault0,
      tokenVault1: addrs.tokenVault1,
      nftTokenProgram,
    };
  }

  /**
   * open_position (v1): NFT mint and NFT account both live under the classic
   * Token program.
   */
  async openPosition(params: OpenPositionParams): Promise<OpenedPosition> {
    const positionNftMint = Keypair.generate();
    const addrs = await this.derivePositionAddresses(
      params,
      positionNftMint.publicKey,
      TOKEN_PROGRAM_ID
    );

    const signature = await this.program.methods
      .openPosition(
        params.tickLowerIndex,
        params.tickUpperIndex,
        addrs.lowerStart,
        addrs.upperStart,
        params.liquidity,
        params.amount0Max,
        params.amount1Max
      )
      .accounts({
        payer: params.payer.publicKey,
        positionNftOwner: params.positionNftOwner,
        positionNftMint: positionNftMint.publicKey,
        // @ts-ignore - positionNftAccount is a PDA derived from positionNftOwner and positionNftMint
        positionNftAccount: addrs.positionNftAccount,
        metadataAccount: addrs.metadataAccount,
        poolState: params.poolState,
        protocolPosition: addrs.protocolPosition,
        // @ts-ignore - tickArrayLower is a PDA derived from poolState and tickArrayLowerStartIndex
        tickArrayLower: addrs.tickArrayLower,
        // @ts-ignore - tickArrayUpper is a PDA derived from poolState and tickArrayUpperStartIndex
        tickArrayUpper: addrs.tickArrayUpper,
        // @ts-ignore - personalPosition is a PDA derived from positionNftMint
        personalPosition: addrs.personalPosition,
        tokenAccount0: addrs.tokenAccount0,
        tokenAccount1: addrs.tokenAccount1,
        tokenVault0: addrs.tokenVault0,
        tokenVault1: addrs.tokenVault1,
        rent: SYSVAR_RENT_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
      } as any)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([params.payer, positionNftMint])
      .rpc();

    return this.toOpenedPosition(
      params,
      addrs,
      positionNftMint.publicKey,
      TOKEN_PROGRAM_ID,
      signature
    );
  }

  /**
   * open_position_v2: same classic-Token NFT mint as v1, but the pool-token
   * vaults go through the Token-2022-aware path.
   */
  async openPositionV2(
    params: OpenPositionParams & {
      withMetadata?: boolean; // Optional, defaults to true
      baseFlag?: boolean | null; // Optional, defaults to null
    }
  ): Promise<OpenedPosition> {
    const positionNftMint = Keypair.generate();
    const addrs = await this.derivePositionAddresses(
      params,
      positionNftMint.publicKey,
      TOKEN_PROGRAM_ID
    );

    const signature = await this.program.methods
      .openPositionV2(
        params.tickLowerIndex,
        params.tickUpperIndex,
        addrs.lowerStart,
        addrs.upperStart,
        params.liquidity,
        params.amount0Max,
        params.amount1Max,
        params.withMetadata ?? true,
        params.baseFlag ?? null
      )
      .accounts({
        payer: params.payer.publicKey,
        positionNftOwner: params.positionNftOwner,
        positionNftMint: positionNftMint.publicKey,
        positionNftAccount: addrs.positionNftAccount,
        metadataAccount: addrs.metadataAccount,
        poolState: params.poolState,
        protocolPosition: addrs.protocolPosition,
        tickArrayLower: addrs.tickArrayLower,
        tickArrayUpper: addrs.tickArrayUpper,
        personalPosition: addrs.personalPosition,
        tokenAccount0: addrs.tokenAccount0,
        tokenAccount1: addrs.tokenAccount1,
        tokenVault0: addrs.tokenVault0,
        tokenVault1: addrs.tokenVault1,
        rent: SYSVAR_RENT_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        metadataProgram: METADATA_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        vault0Mint: params.tokenVault0Mint,
        vault1Mint: params.tokenVault1Mint,
      } as any)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([params.payer, positionNftMint])
      .rpc();

    return this.toOpenedPosition(
      params,
      addrs,
      positionNftMint.publicKey,
      TOKEN_PROGRAM_ID,
      signature
    );
  }

  /**
   * open_position_with_token22_nft: the NFT mint and NFT account themselves
   * live under Token-2022, so there is no Metaplex metadata account.
   */
  async openPositionWithToken22Nft(
    params: OpenPositionParams & {
      withMetadata?: boolean; // Optional, defaults to false
      baseFlag?: boolean | null; // Optional, defaults to null
    }
  ): Promise<OpenedPosition> {
    const positionNftMint = Keypair.generate();
    const addrs = await this.derivePositionAddresses(
      params,
      positionNftMint.publicKey,
      TOKEN_2022_PROGRAM_ID
    );

    const signature = await this.program.methods
      .openPositionWithToken22Nft(
        params.tickLowerIndex,
        params.tickUpperIndex,
        addrs.lowerStart,
        addrs.upperStart,
        params.liquidity,
        params.amount0Max,
        params.amount1Max,
        params.withMetadata ?? false,
        params.baseFlag ?? null
      )
      .accounts({
        payer: params.payer.publicKey,
        positionNftOwner: params.positionNftOwner,
        positionNftMint: positionNftMint.publicKey,
        positionNftAccount: addrs.positionNftAccount,
        poolState: params.poolState,
        protocolPosition: addrs.protocolPosition,
        tickArrayLower: addrs.tickArrayLower,
        tickArrayUpper: addrs.tickArrayUpper,
        personalPosition: addrs.personalPosition,
        tokenAccount0: addrs.tokenAccount0,
        tokenAccount1: addrs.tokenAccount1,
        tokenVault0: addrs.tokenVault0,
        tokenVault1: addrs.tokenVault1,
        rent: SYSVAR_RENT_PUBKEY,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        vault0Mint: params.tokenVault0Mint,
        vault1Mint: params.tokenVault1Mint,
      } as any)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([params.payer, positionNftMint])
      .rpc();

    return this.toOpenedPosition(
      params,
      addrs,
      positionNftMint.publicKey,
      TOKEN_2022_PROGRAM_ID,
      signature
    );
  }

  /**
   * Winds a position all the way down the way a real client would:
   * decrease_liquidity_v2 to zero (skipped when it already is), then
   * close_position. Works for both classic-Token and Token-2022 position NFTs,
   * and for a frozen NFT account this exercises close_position's
   * thaw -> burn -> close sequence.
   */
  async closePosition(params: { owner: Keypair; position: OpenedPosition }) {
    const { owner, position } = params;
    const personalPositionData =
      await this.program.account.personalPositionState.fetch(
        position.personalPosition
      );

    if (!personalPositionData.liquidity.isZero()) {
      const recipientTokenAccount0 = getAssociatedTokenAddressSync(
        position.token0,
        owner.publicKey
      );
      const recipientTokenAccount1 = getAssociatedTokenAddressSync(
        position.token1,
        owner.publicKey
      );

      await this.program.methods
        .decreaseLiquidityV2(
          personalPositionData.liquidity,
          new anchor.BN(0),
          new anchor.BN(0)
        )
        .accounts({
          nftOwner: owner.publicKey,
          nftAccount: position.positionNftAccount,
          personalPosition: position.personalPosition,
          poolState: position.poolState,
          protocolPosition: position.protocolPosition,
          tokenVault0: position.tokenVault0,
          tokenVault1: position.tokenVault1,
          tickArrayLower: position.tickArrayLower,
          tickArrayUpper: position.tickArrayUpper,
          recipientTokenAccount0,
          recipientTokenAccount1,
          tokenProgram: TOKEN_PROGRAM_ID,
          tokenProgram2022: TOKEN_2022_PROGRAM_ID,
          memoProgram: MEMO_PROGRAM_ID,
          vault0Mint: position.token0,
          vault1Mint: position.token1,
        } as any)
        .signers([owner])
        .rpc();
    }

    return await this.program.methods
      .closePosition()
      .accounts({
        nftOwner: owner.publicKey,
        positionNftMint: position.positionNftMint,
        positionNftAccount: position.positionNftAccount,
        personalPosition: position.personalPosition,
        systemProgram: SystemProgram.programId,
        tokenProgram: position.nftTokenProgram,
      } as any)
      // The pool is the NFT mint's freeze authority. close_position only reads it
      // when the NFT account is frozen, so it rides in remaining_accounts rather
      // than the declared account list, leaving the IDL untouched.
      .remainingAccounts([
        { pubkey: position.poolState, isWritable: false, isSigner: false },
      ])
      .signers([owner])
      .rpc();
  }

  /**
   * create_pool for an explicit mint pair. `TestSetup.createPool` covers the
   * shared-fixture case; use this when a test needs its own pair (for example
   * mints carrying a particular freeze authority).
   */
  async createPool(params: {
    poolCreator: Keypair;
    ammConfig: PublicKey;
    tokenMint0: PublicKey;
    tokenMint1: PublicKey;
    tick?: number; // Optional, defaults to 0
    openTime?: anchor.BN; // Optional, defaults to 100s ago
  }): Promise<PublicKey> {
    const [poolState] = await this.pda.getPoolStatePDA(
      params.ammConfig,
      params.tokenMint0,
      params.tokenMint1
    );
    const [tokenVault0] = await this.pda.getTokenVaultPDA(
      poolState,
      params.tokenMint0
    );
    const [tokenVault1] = await this.pda.getTokenVaultPDA(
      poolState,
      params.tokenMint1
    );
    const [observationState] = await this.pda.getObservationStatePDA(poolState);
    const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(poolState);
    const openTime =
      params.openTime ?? new anchor.BN(Math.floor(Date.now() / 1000) - 100);

    await this.program.methods
      .createPool(TickUtil.getSqrtPriceAtTick(params.tick ?? 0), openTime)
      .accounts({
        poolCreator: params.poolCreator.publicKey,
        ammConfig: params.ammConfig,
        poolState: poolState,
        tokenMint0: params.tokenMint0,
        tokenMint1: params.tokenMint1,
        tokenVault0: tokenVault0,
        tokenVault1: tokenVault1,
        observationState: observationState,
        tickArrayBitmap: tickArrayBitmap,
        tokenProgram0: TOKEN_PROGRAM_ID,
        tokenProgram1: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .signers([params.poolCreator])
      .rpc();

    return poolState;
  }

  /**
   * Swap helper function
   */
  async swapV2(
    params: {
      owner: Keypair;
      ammConfig: PublicKey;
      poolState: PublicKey;
      inputVaultMint: PublicKey;
      outputVaultMint: PublicKey;
      amount: anchor.BN;
      otherAmountThreshold: anchor.BN;
      sqrtPriceLimitX64: anchor.BN;
      isBaseInput: boolean;
      remainingAccounts: AccountMeta[];
    },
    options?: {
      skipPreflight?: boolean;
    }
  ) {
    // Derive observation state for observer account
    const [observationState] = await this.pda.getObservationStatePDA(
      params.poolState
    );

    // Derive input vault
    const [inputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.inputVaultMint
    );

    // Derive output vault
    const [outputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.outputVaultMint
    );
    const inputTokenAccount = getAssociatedTokenAddressSync(
      params.inputVaultMint,
      params.owner.publicKey
    );
    const outputTokenAccount = getAssociatedTokenAddressSync(
      params.outputVaultMint,
      params.owner.publicKey
    );

    return await this.program.methods
      .swapV2(
        params.amount,
        params.otherAmountThreshold,
        params.sqrtPriceLimitX64,
        params.isBaseInput
      )
      .accounts({
        payer: params.owner.publicKey,
        ammConfig: params.ammConfig,
        poolState: params.poolState,
        inputTokenAccount: inputTokenAccount,
        outputTokenAccount: outputTokenAccount,
        inputVault: inputVault,
        outputVault: outputVault,
        observationState: observationState,
        inputVaultMint: params.inputVaultMint,
        outputVaultMint: params.outputVaultMint,
      })
      .remainingAccounts(params.remainingAccounts)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1400000 }),
      ])
      .signers([params.owner])
      .rpc(options);
  }

  /**
   * Open Limit Order helper function
   */
  async openLimitOrder(params: {
    owner: Keypair;
    poolState: PublicKey;
    tickIndex: number;
    zeroForOne: boolean;
    amount: anchor.BN;
    nonceIndex?: number;
  }) {
    const nonceIndex = params.nonceIndex ?? 0;

    // Derive limit_order_nonce PDA
    const [limitOrderNonce] = await this.pda.getLimitOrderNoncePDA(
      params.owner.publicKey,
      nonceIndex
    );

    // Fetch current order_nonce from the nonce account (0 if not yet created)
    let orderNonce = new anchor.BN(0);
    try {
      const nonceData = await this.program.account.limitOrderNonce.fetch(
        limitOrderNonce
      );
      orderNonce = nonceData.orderNonce as anchor.BN;
    } catch (_e) {
      // Account doesn't exist yet, nonce starts at 0
    }

    // Derive limit_order PDA using new seeds: [owner, limit_order_nonce, order_nonce]
    const [limitOrder] = await this.pda.getLimitOrderStatePDA(
      params.owner.publicKey,
      limitOrderNonce,
      orderNonce
    );

    // Load pool state to get tick spacing and token mints
    const poolStateData = await this.program.account.poolState.fetch(
      params.poolState
    );

    // Calculate tick array start index using tick spacing
    const tickSpacing = poolStateData.tickSpacing;
    const tickArray = getTickArrayAddressByTick(
      this.program.programId,
      params.poolState,
      params.tickIndex,
      tickSpacing
    );
    const inputVaultMint = params.zeroForOne
      ? poolStateData.tokenMint0
      : poolStateData.tokenMint1;

    // Derive input vault using PDA utils
    const [inputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      inputVaultMint
    );

    // Derive owner's input token account (ATA)
    const inputTokenAccount = getAssociatedTokenAddressSync(
      inputVaultMint,
      params.owner.publicKey
    );

    // Determine input token program from mint owner
    const inputVaultMintInfo = await this.provider.connection.getAccountInfo(
      inputVaultMint
    );
    const inputTokenProgram = inputVaultMintInfo?.owner.equals(
      TOKEN_2022_PROGRAM_ID
    )
      ? TOKEN_2022_PROGRAM_ID
      : TOKEN_PROGRAM_ID;

    const outputVaultMint = params.zeroForOne
      ? poolStateData.tokenMint1
      : poolStateData.tokenMint0;

    const [outputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      outputVaultMint
    );

    const outputTokenAccount = getAssociatedTokenAddressSync(
      outputVaultMint,
      params.owner.publicKey
    );

    const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(
      params.poolState
    );

    const tx = await this.program.methods
      .openLimitOrder(
        nonceIndex,
        params.zeroForOne,
        params.tickIndex,
        params.amount
      )
      .accounts({
        payer: params.owner.publicKey,
        poolState: params.poolState,
        tickArray: tickArray,
        limitOrderNonce: limitOrderNonce,
        limitOrder: limitOrder,
        inputTokenAccount: inputTokenAccount,
        outputTokenAccount: outputTokenAccount,
        inputVault: inputVault,
        outputVault: outputVault,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        inputTokenProgram: inputTokenProgram,
        systemProgram: SystemProgram.programId,
      } as any)
      .remainingAccounts([
        { pubkey: tickArrayBitmap, isWritable: true, isSigner: false },
      ])
      .signers([params.owner])
      .rpc();

    return {
      signature: tx,
      limitOrder: limitOrder,
    };
  }

  /**
   * Increase Limit Order helper function
   */
  async increaseLimitOrder(params: {
    owner: Keypair;
    poolState: PublicKey;
    limitOrder: PublicKey;
    amount: anchor.BN;
  }) {
    // Load limit order and pool state to get required info
    const limitOrderData = await this.program.account.limitOrderState.fetch(
      params.limitOrder
    );
    const poolStateData = await this.program.account.poolState.fetch(
      params.poolState
    );

    // Calculate tick array start index from limit order's tick index
    const tickSpacing = poolStateData.tickSpacing;
    const tickArrayStartIndex = TickArrayUtil.getTickArrayStartIndex(
      limitOrderData.tickIndex,
      tickSpacing
    );
    const [tickArray] = await this.pda.getTickArrayStatePDA(
      params.poolState,
      tickArrayStartIndex
    );

    const inputVaultMint = limitOrderData.zeroForOne
      ? poolStateData.tokenMint0
      : poolStateData.tokenMint1;

    // Derive input vault
    const [inputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      inputVaultMint
    );

    // Derive owner's input token account (ATA)
    const inputTokenAccount = getAssociatedTokenAddressSync(
      inputVaultMint,
      params.owner.publicKey
    );

    // Determine which token program to use based on mint owner
    // This should match the token program that owns the input_vault_mint
    // For simplicity, we check if it's TOKEN_2022 by checking the mint
    // In practice, we might need to fetch the mint account to determine the program
    // For now, we'll use TOKEN_PROGRAM_ID as default, but this should be determined dynamically
    const mintInfo = await this.program.provider.connection.getAccountInfo(
      inputVaultMint
    );
    const inputTokenProgram = mintInfo?.owner.equals(TOKEN_2022_PROGRAM_ID)
      ? TOKEN_2022_PROGRAM_ID
      : TOKEN_PROGRAM_ID;

    return await this.program.methods
      .increaseLimitOrder(params.amount)
      .accounts({
        owner: params.owner.publicKey,
        poolState: params.poolState,
        tickArray: tickArray,
        limitOrder: params.limitOrder,
        inputTokenAccount: inputTokenAccount,
        inputVault: inputVault,
        inputVaultMint: inputVaultMint,
        inputTokenProgram: inputTokenProgram,
      } as any)
      .signers([params.owner])
      .rpc();
  }

  /**
   * Decrease Limit Order helper function
   */
  async decreaseLimitOrder(params: {
    owner: Keypair;
    poolState: PublicKey;
    limitOrder: PublicKey;
    amount: anchor.BN;
    amountMin: anchor.BN;
  }) {
    // Load limit order and pool state to get required info
    const limitOrderData = await this.program.account.limitOrderState.fetch(
      params.limitOrder
    );
    const poolStateData = await this.program.account.poolState.fetch(
      params.poolState
    );

    // Calculate tick array start index from limit order's tick index
    const tickSpacing = poolStateData.tickSpacing;
    const tickArrayStartIndex = TickArrayUtil.getTickArrayStartIndex(
      limitOrderData.tickIndex,
      tickSpacing
    );
    const [tickArray] = await this.pda.getTickArrayStatePDA(
      params.poolState,
      tickArrayStartIndex
    );

    const inputVaultMint = limitOrderData.zeroForOne
      ? poolStateData.tokenMint0
      : poolStateData.tokenMint1;
    const outputVaultMint = limitOrderData.zeroForOne
      ? poolStateData.tokenMint1
      : poolStateData.tokenMint0;

    // Derive input and output vaults using PDA utils
    const [inputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      inputVaultMint
    );
    const [outputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      outputVaultMint
    );

    // Derive owner's input and output token accounts (ATA)
    const inputTokenAccount = getAssociatedTokenAddressSync(
      inputVaultMint,
      params.owner.publicKey
    );
    const outputTokenAccount = getAssociatedTokenAddressSync(
      outputVaultMint,
      params.owner.publicKey
    );

    const ticksInArray = 60 * tickSpacing;
    // Check if tick array bitmap extension is needed
    const maxTickBoundary = ticksInArray * 512;
    const minTickBoundary = -maxTickBoundary;
    const isOverflowDefaultBitmap =
      tickArrayStartIndex >= maxTickBoundary ||
      tickArrayStartIndex < minTickBoundary;

    // Build remaining accounts if needed
    const remainingAccounts: AccountMeta[] = [];
    if (isOverflowDefaultBitmap) {
      const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(
        params.poolState
      );
      remainingAccounts.push({
        pubkey: tickArrayBitmap,
        isSigner: false,
        isWritable: true,
      });
    }

    let instructionBuilder = this.program.methods
      .decreaseLimitOrder(params.amount, params.amountMin)
      .accounts({
        owner: params.owner.publicKey,
        poolState: params.poolState,
        tickArray: tickArray,
        limitOrder: params.limitOrder,
        inputTokenAccount: inputTokenAccount,
        outputTokenAccount: outputTokenAccount,
        inputVault: inputVault,
        outputVault: outputVault,
        inputVaultMint: inputVaultMint,
        outputVaultMint: outputVaultMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
      } as any);

    if (remainingAccounts.length > 0) {
      instructionBuilder =
        instructionBuilder.remainingAccounts(remainingAccounts);
    }

    return await instructionBuilder.signers([params.owner]).rpc();
  }

  /**
   * Settle Limit Order helper function
   */
  async settleLimitOrder(params: {
    owner: Keypair;
    poolState: PublicKey;
    limitOrder: PublicKey;
  }) {
    // Load limit order and pool state to get required info
    const limitOrderData = await this.program.account.limitOrderState.fetch(
      params.limitOrder
    );
    const poolStateData = await this.program.account.poolState.fetch(
      params.poolState
    );

    // Calculate tick array start index from limit order's tick index
    const tickSpacing = poolStateData.tickSpacing;
    const tickArrayStartIndex = TickArrayUtil.getTickArrayStartIndex(
      limitOrderData.tickIndex,
      tickSpacing
    );
    const [tickArray] = await this.pda.getTickArrayStatePDA(
      params.poolState,
      tickArrayStartIndex
    );

    const outputVaultMint = limitOrderData.zeroForOne
      ? poolStateData.tokenMint1
      : poolStateData.tokenMint0;

    // Derive output vault using PDA utils
    const [outputVault] = await this.pda.getTokenVaultPDA(
      params.poolState,
      outputVaultMint
    );

    // Derive owner's output token account (ATA)
    const outputTokenAccount = getAssociatedTokenAddressSync(
      outputVaultMint,
      limitOrderData.owner
    );

    // Determine output token program from mint owner
    const outputVaultMintInfo = await this.provider.connection.getAccountInfo(
      outputVaultMint
    );
    const outputTokenProgram = outputVaultMintInfo?.owner.equals(
      TOKEN_2022_PROGRAM_ID
    )
      ? TOKEN_2022_PROGRAM_ID
      : TOKEN_PROGRAM_ID;

    return await this.program.methods
      .settleLimitOrder()
      .accounts({
        signer: params.owner.publicKey,
        poolState: params.poolState,
        tickArray: tickArray,
        limitOrder: params.limitOrder,
        outputTokenAccount: outputTokenAccount,
        outputVault: outputVault,
        outputVaultMint: outputVaultMint,
        outputTokenProgram: outputTokenProgram,
      } as any)
      .signers([params.owner])
      .rpc({ skipPreflight: true });
  }

  /**
   * Close Limit Order helper function
   * Closes the limit order account when unfilled amount is zero
   */
  async closeLimitOrder(params: { owner: Keypair; limitOrder: PublicKey }) {
    // Load limit order to get owner for rent receiver
    const limitOrderData = await this.program.account.limitOrderState.fetch(
      params.limitOrder
    );

    // Rent receiver is the limit order owner
    const rentReceiver = limitOrderData.owner;

    return await (this.program.methods as any)
      .closeLimitOrder()
      .accounts({
        signer: params.owner.publicKey,
        rentReceiver: rentReceiver,
        limitOrder: params.limitOrder,
      } as any)
      .signers([params.owner])
      .rpc();
  }

  /**
   * Create Dynamic Fee Config helper function
   */
  async createDynamicFeeConfig(params: {
    payer: Keypair;
    index: number;
    filterPeriod: number;
    decayPeriod: number;
    reductionFactor: number;
    dynamicFeeControl: number; // Note: IDL uses 'dynamic_fee_control', not 'dynamic_fee_control_factor'
    maxVolatilityAccumulator: number;
  }) {
    const [dynamicFeeConfig] = await this.pda.getDynamicFeeConfigPDA(
      params.index
    );

    return await this.program.methods
      .createDynamicFeeConfig(
        params.index,
        params.filterPeriod,
        params.decayPeriod,
        params.reductionFactor,
        params.dynamicFeeControl,
        params.maxVolatilityAccumulator
      )
      .accounts({
        owner: params.payer.publicKey,
        dynamicFeeConfig: dynamicFeeConfig,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([params.payer])
      .rpc();
  }

  async createCustomizablePool(params: {
    payer: Keypair;
    ammConfig: PublicKey;
    poolState: PublicKey;
    tokenMint0: PublicKey;
    tokenMint1: PublicKey;
    sqrtPriceX64: anchor.BN;
    collectFeeOn?:
      | { fromInput?: {} }
      | { token0Only?: {} }
      | { token1Only?: {} }; // Optional, defaults to fromInput
    enableDynamicFee?: boolean; // Optional, defaults to false
    dynamicFeeConfig?: PublicKey; // Optional, only needed if enableDynamicFee is true
  }) {
    const [tokenVault0] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.tokenMint0
    );
    const [tokenVault1] = await this.pda.getTokenVaultPDA(
      params.poolState,
      params.tokenMint1
    );
    const [observationState] = await this.pda.getObservationStatePDA(
      params.poolState
    );
    const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(
      params.poolState
    );

    // Default to fromInput if not specified
    const collectFeeOn = params.collectFeeOn || { fromInput: {} };
    const enableDynamicFee = params.enableDynamicFee || false;

    // If dynamic fee is enabled, dynamic_fee_config must be in remaining_accounts
    const remainingAccounts =
      enableDynamicFee && params.dynamicFeeConfig
        ? [
            {
              pubkey: params.dynamicFeeConfig,
              isWritable: false,
              isSigner: false,
            },
          ]
        : [];

    return await this.program.methods
      .createCustomizablePool({
        sqrtPriceX64: params.sqrtPriceX64,
        collectFeeOn: collectFeeOn as any, // Type assertion needed for enum
        enableDynamicFee: enableDynamicFee,
      })
      .accounts({
        poolCreator: params.payer.publicKey,
        ammConfig: params.ammConfig,
        poolState: params.poolState,
        tokenMint0: params.tokenMint0,
        tokenMint1: params.tokenMint1,
        tokenVault0: tokenVault0,
        tokenVault1: tokenVault1,
        observationState: observationState,
        tickArrayBitmap: tickArrayBitmap,
        tokenProgram0: TOKEN_PROGRAM_ID,
        tokenProgram1: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: SYSVAR_RENT_PUBKEY,
      } as any)
      .remainingAccounts(remainingAccounts)
      .signers([params.payer])
      .rpc();
  }
}
