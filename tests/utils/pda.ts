import * as anchor from "@coral-xyz/anchor";
import { PublicKey } from "@solana/web3.js";

/**
 * PDA (Program Derived Address) utilities for Raydium CLMM
 * Centralizes all PDA calculations for reuse across tests
 */
export class PDAUtils {
  private programId: PublicKey;

  constructor(programId: PublicKey) {
    this.programId = programId;
  }

  /**
   * Get AMM Config PDA
   * Seeds: ["amm_config", index_bytes]
   */
  async getAmmConfigPDA(index: number): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("amm_config"), Buffer.from([index >> 8, index & 0xff])],
      this.programId,
    );
  }

  /**
   * Get Pool State PDA
   * Seeds: ["pool", amm_config, token_mint_0, token_mint_1]
   */
  async getPoolStatePDA(
    ammConfig: PublicKey,
    tokenMint0: PublicKey,
    tokenMint1: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [
        Buffer.from("pool"),
        ammConfig.toBuffer(),
        tokenMint0.toBuffer(),
        tokenMint1.toBuffer(),
      ],
      this.programId,
    );
  }

  /**
   * Get Token Vault PDA
   * Seeds: ["pool_vault", pool_state, token_mint]
   */
  async getTokenVaultPDA(
    poolState: PublicKey,
    tokenMint: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("pool_vault"), poolState.toBuffer(), tokenMint.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Observation State PDA
   * Seeds: ["observation", pool_state]
   */
  async getObservationStatePDA(
    poolState: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("observation"), poolState.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Tick Array Bitmap PDA
   * Seeds: ["pool_tick_array_bitmap_extension", pool_state]
   */
  async getTickArrayBitmapPDA(
    poolState: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("pool_tick_array_bitmap_extension"), poolState.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Personal Position State PDA
   * Seeds: ["position", nft_mint]
   */
  async getPersonalPositionStatePDA(
    nftMint: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("position"), nftMint.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Protocol Position State PDA
   * Seeds: ["protocol_position", pool_state, tick_lower_index, tick_upper_index]
   */
  async getProtocolPositionStatePDA(
    poolState: PublicKey,
    tickLowerIndex: number,
    tickUpperIndex: number,
  ): Promise<[PublicKey, number]> {
    const tickLowerBuffer = Buffer.alloc(4);
    tickLowerBuffer.writeInt32BE(tickLowerIndex, 0); // Use BE to match contract
    const tickUpperBuffer = Buffer.alloc(4);
    tickUpperBuffer.writeInt32BE(tickUpperIndex, 0); // Use BE to match contract

    return await PublicKey.findProgramAddress(
      [
        Buffer.from("protocol_position"),
        poolState.toBuffer(),
        tickLowerBuffer,
        tickUpperBuffer,
      ],
      this.programId,
    );
  }

  /**
   * Get Tick Array State PDA
   * Seeds: ["tick_array", pool_state, start_tick_index]
   */
  async getTickArrayStatePDA(
    poolState: PublicKey,
    startTickIndex: number,
  ): Promise<[PublicKey, number]> {
    const startTickBuffer = Buffer.alloc(4);
    startTickBuffer.writeInt32BE(startTickIndex, 0); // Use BE (big-endian) to match contract

    return await PublicKey.findProgramAddress(
      [Buffer.from("tick_array"), poolState.toBuffer(), startTickBuffer],
      this.programId,
    );
  }

  /**
   * Get Limit Order Nonce PDA
   * Seeds: [owner, nonce_index (u8)]
   */
  async getLimitOrderNoncePDA(
    owner: PublicKey,
    nonceIndex: number,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [owner.toBuffer(), Buffer.from([nonceIndex])],
      this.programId,
    );
  }

  /**
   * Get Limit Order State PDA
   * Seeds: [owner, limit_order_nonce, order_nonce (u64 BE)]
   */
  async getLimitOrderStatePDA(
    owner: PublicKey,
    limitOrderNonce: PublicKey,
    orderNonce: anchor.BN,
  ): Promise<[PublicKey, number]> {
    const orderNonceBuffer = Buffer.alloc(8);
    orderNonceBuffer.writeBigUInt64BE(BigInt(orderNonce.toString()), 0);
    return await PublicKey.findProgramAddress(
      [
        owner.toBuffer(),
        limitOrderNonce.toBuffer(),
        orderNonceBuffer,
      ],
      this.programId,
    );
  }

  /**
   * Get Operation State PDA
   * Seeds: ["operation", pool_state]
   */
  async getOperationStatePDA(
    poolState: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("operation"), poolState.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Pool Reward Vault PDA
   * Seeds: ["pool_reward_vault", pool_state, reward_mint]
   */
  async getPoolRewardVaultPDA(
    poolState: PublicKey,
    rewardMint: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [
        Buffer.from("pool_reward_vault"),
        poolState.toBuffer(),
        rewardMint.toBuffer(),
      ],
      this.programId,
    );
  }

  /**
   * Get Support Mint Associated PDA
   * Seeds: ["support_mint_associated", mint]
   */
  async getSupportMintAssociatedPDA(
    mint: PublicKey,
  ): Promise<[PublicKey, number]> {
    return await PublicKey.findProgramAddress(
      [Buffer.from("support_mint_associated"), mint.toBuffer()],
      this.programId,
    );
  }

  /**
   * Get Dynamic Fee Config PDA
   * Seeds: ["dynamic_fee_config", index_bytes]
   */
  async getDynamicFeeConfigPDA(index: number): Promise<[PublicKey, number]> {
    const indexBuffer = Buffer.alloc(2);
    indexBuffer.writeUInt16BE(index, 0);
    return await PublicKey.findProgramAddress(
      [Buffer.from("dynamic_fee_config"), indexBuffer],
      this.programId,
    );
  }
}

/**
 * Convenience function to create PDAUtils instance
 */
export function createPDAUtils(programId: PublicKey): PDAUtils {
  return new PDAUtils(programId);
}
