import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../../target/types/raydium_clmm";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
} from "@solana/spl-token";
import { SqrtPriceMath } from "@raydium-io/raydium-sdk-v2";
import { PDAUtils } from "./pda";

/**
 * Pool State data structure (TypeScript interface)
 * Matches the Rust PoolState struct
 */
export interface PoolStateData {
  bump: number[];
  ammConfig: PublicKey;
  owner: PublicKey;
  tokenMint0: PublicKey;
  tokenMint1: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  observationKey: PublicKey;
  mintDecimals0: number;
  mintDecimals1: number;
  tickSpacing: number;
  liquidity: anchor.BN;
  sqrtPriceX64: anchor.BN;
  tickCurrent: number;
  padding3: number;
  padding4: number;
  feeGrowthGlobal0X64: anchor.BN;
  feeGrowthGlobal1X64: anchor.BN;
  protocolFeesToken0: anchor.BN;
  protocolFeesToken1: anchor.BN;
  swapInAmountToken0: anchor.BN;
  swapOutAmountToken1: anchor.BN;
  swapInAmountToken1: anchor.BN;
  swapOutAmountToken0: anchor.BN;
  status: number;
  padding: number[];
  rewardInfos: any[];
  tickArrayBitmap: anchor.BN[];
  totalFeesToken0: anchor.BN;
  totalFeesClaimedToken0: anchor.BN;
  totalFeesToken1: anchor.BN;
  totalFeesClaimedToken1: anchor.BN;
  fundFeesToken0: anchor.BN;
  fundFeesToken1: anchor.BN;
  openTime: anchor.BN;
  recentEpoch: anchor.BN;
  limitTotalFeesToken0: anchor.BN;
  limitTotalFeesToken1: anchor.BN;
  limitOrderFeeGrowthGlobal0X64: anchor.BN;
  limitOrderFeeGrowthGlobal1X64: anchor.BN;
  padding1: anchor.BN[];
  padding2: anchor.BN[];
}

/**
 * Test setup utilities
 */
export class TestSetup {
  program: Program<RaydiumClmm>;
  provider: anchor.AnchorProvider;
  admin: Keypair;
  pda: PDAUtils;

  // Token mints
  token0: PublicKey;
  token1: PublicKey;

  // Token accounts
  token0Account: PublicKey;
  token1Account: PublicKey;

  ammConfig: PublicKey;
  poolAddress: PublicKey;

  // Pool information structure
  pool: PoolStateData | null;

  constructor(program: Program<RaydiumClmm>, admin: Keypair) {
    this.program = program;
    this.provider = anchor.AnchorProvider.env();
    anchor.setProvider(this.provider);
    this.admin = admin;
    this.pool = null;
    this.pda = new PDAUtils(program.programId);
  }

  async initialize() {
    const airdropSig = await this.provider.connection.requestAirdrop(
      this.admin.publicKey,
      10 * anchor.web3.LAMPORTS_PER_SOL
    );
    await this.provider.connection.confirmTransaction(airdropSig);
    // console.log("Admin initialized:", this.admin.publicKey.toString());
  }

  async createTokens() {
    this.token0 = await createMint(
      this.provider.connection,
      this.admin,
      this.admin.publicKey,
      null,
      9
    );

    this.token1 = await createMint(
      this.provider.connection,
      this.admin,
      this.admin.publicKey,
      null,
      9
    );

    // Ensure token0 < token1
    if (this.token0.toBuffer().compare(this.token1.toBuffer()) > 0) {
      const temp = this.token0;
      this.token0 = this.token1;
      this.token1 = temp;
    }

    // console.log(
    //   "Tokens created:",
    //   this.token0.toString(),
    //   this.token1.toString()
    // );
  }

  async mintTokens(amount: anchor.BN = new anchor.BN(1_000_000_000_000_000)) {
    this.token0Account = await createAccount(
      this.provider.connection,
      this.admin,
      this.token0,
      this.admin.publicKey
    );

    this.token1Account = await createAccount(
      this.provider.connection,
      this.admin,
      this.token1,
      this.admin.publicKey
    );

    await mintTo(
      this.provider.connection,
      this.admin,
      this.token0,
      this.token0Account,
      this.admin,
      amount.toNumber()
    );

    await mintTo(
      this.provider.connection,
      this.admin,
      this.token1,
      this.token1Account,
      this.admin,
      amount.toNumber()
    );
  }

  async createAmmConfig(ammConfigIndex?: number) {
    const index = ammConfigIndex ?? 0;
    const [ammConfig] = await this.pda.getAmmConfigPDA(index);
    this.ammConfig = ammConfig;
    // console.log("AMM config address:", ammConfig.toString());
    try {
      const ammConfigData = await this.program.account.ammConfig.fetch(
        ammConfig
      );
      // console.log("AMM config", ammConfigData);
      return ammConfig;
    } catch (err) {
      console.log("Creating new AMM config...");

      const tx = await this.program.methods
        .createAmmConfig(
          0,
          10, // tickSpacing
          2500, // tradeFeeRate: 0.25%
          120000, // protocolFeeRate: 12%
          0 // fundFeeRate
        )
        .accounts({
          admin: this.admin.publicKey,
          ammConfig: ammConfig,
          systemProgram: SystemProgram.programId,
        })
        .signers([this.admin])
        .rpc();

      console.log("AMM config created:", tx);
      return ammConfig;
    }
  }

  async createPool(tick: number, ammConfigIndex?: number) {
    await this.createTokens();
    await this.mintTokens();
    // Create AMM config and pool for testing
    await this.createAmmConfig(ammConfigIndex);

    const sqrtPriceX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tick);

    const [poolAddress] = await this.pda.getPoolStatePDA(
      this.ammConfig,
      this.token0,
      this.token1
    );
    this.poolAddress = poolAddress;
    // console.log("Pool address:", poolAddress.toString());
    const [tokenVault0] = await this.pda.getTokenVaultPDA(
      poolAddress,
      this.token0
    );
    const [tokenVault1] = await this.pda.getTokenVaultPDA(
      poolAddress,
      this.token1
    );
    const [observationState] = await this.pda.getObservationStatePDA(
      poolAddress
    );
    const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(poolAddress);

    try {
      const poolStateData = await this.program.account.poolState.fetch(
        poolAddress
      );
      console.log("Pool already exists, skipping creation");
      this.pool = poolStateData as any as PoolStateData;
      return poolAddress;
    } catch (err) {
      const openTime = new anchor.BN(Math.floor(Date.now() / 1000) - 100);

      const tx = await this.program.methods
        .createPool(sqrtPriceX64, openTime)
        .accounts({
          poolCreator: this.admin.publicKey,
          ammConfig: this.ammConfig,
          poolState: poolAddress,
          tokenMint0: this.token0,
          tokenMint1: this.token1,
          tokenVault0: tokenVault0,
          tokenVault1: tokenVault1,
          observationState: observationState,
          tickArrayBitmap: tickArrayBitmap,
          tokenProgram0: TOKEN_PROGRAM_ID,
          tokenProgram1: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        } as any)
        .signers([this.admin])
        .rpc();

      // console.log("Pool created:", tx);

      const poolStateData = await this.program.account.poolState.fetch(
        poolAddress
      );
      this.pool = poolStateData as any as PoolStateData;
      this.poolAddress = poolAddress;
      return poolAddress;
    }
  }

  async createCustomizablePool(params: {
    tick?: number; // Optional, defaults to 0
    ammConfig?: PublicKey; // Optional, uses this.ammConfig if not provided
    collectFeeOn: { fromInput?: {} } | { token0Only?: {} } | { token1Only?: {} };
    enableDynamicFee: boolean;
    dynamicFeeConfig?: PublicKey; // Optional, only needed if enableDynamicFee is true
  }): Promise<PublicKey> {
    // Ensure tokens are created
    if (!this.token0 || !this.token1) {
      await this.createTokens();
    }
    await this.mintTokens();

    // Ensure AMM config exists
    if (!this.ammConfig) {
      await this.createAmmConfig();
    }

    const tick = params.tick ?? 0;
    const sqrtPriceX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tick);
    const ammConfig = params.ammConfig ?? this.ammConfig;

    // Ensure token0 < token1
    const [finalTokenMint0, finalTokenMint1] =
      this.token0.toBuffer().compare(this.token1.toBuffer()) < 0
        ? [this.token0, this.token1]
        : [this.token1, this.token0];

    const [poolAddress] = await this.pda.getPoolStatePDA(
      ammConfig,
      finalTokenMint0,
      finalTokenMint1
    );

    try {
      const poolStateData = await this.program.account.poolState.fetch(
        poolAddress
      );
      console.log("Pool already exists, skipping creation");
      this.pool = poolStateData as unknown as PoolStateData;
      this.poolAddress = poolAddress;
      return poolAddress;
    } catch (err) {
      const [tokenVault0] = await this.pda.getTokenVaultPDA(
        poolAddress,
        finalTokenMint0
      );
      const [tokenVault1] = await this.pda.getTokenVaultPDA(
        poolAddress,
        finalTokenMint1
      );
      const [observationState] = await this.pda.getObservationStatePDA(
        poolAddress
      );
      const [tickArrayBitmap] = await this.pda.getTickArrayBitmapPDA(
        poolAddress
      );

      const collectFeeOn = params.collectFeeOn;
      const enableDynamicFee = params.enableDynamicFee;

      // If dynamic fee is enabled, dynamic_fee_config must be in remaining_accounts
      const remainingAccounts = enableDynamicFee && params.dynamicFeeConfig
        ? [{ pubkey: params.dynamicFeeConfig, isWritable: false, isSigner: false }]
        : [];

      await this.program.methods
        .createCustomizablePool({
          sqrtPriceX64: sqrtPriceX64,
          collectFeeOn: collectFeeOn as any, // Type assertion needed for enum
          enableDynamicFee: enableDynamicFee,
        })
        .accounts({
          poolCreator: this.admin.publicKey,
          ammConfig: ammConfig,
          poolState: poolAddress,
          tokenMint0: finalTokenMint0,
          tokenMint1: finalTokenMint1,
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
        .signers([this.admin])
        .rpc();

      const poolStateData = await this.program.account.poolState.fetch(
        poolAddress
      );
      this.pool = poolStateData as any as PoolStateData;
      this.poolAddress = poolAddress;
      return poolAddress;
    }
  }
}
