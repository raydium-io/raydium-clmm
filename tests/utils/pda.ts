import { PublicKey } from "@solana/web3.js";

import {
  AMM_CONFIG_SEED,
  POOL_SEED,
  POOL_VAULT_SEED,
  FEE_SEED,
  POSITION_SEED,
  TICK_SEED,
  BITMAP_SEED,
  OBSERVATION_SEED,
  POOL_REWARD_VAULT_SEED,
  u32ToBytes,
  u16ToBytes,
  i32ToBytes,
  i16ToBytes
} from "./seed";

import * as metaplex from "@metaplex/js";
const {
    metadata: { Metadata },
  } = metaplex.programs;

export async function getAmmConfigAddress(
  programId: PublicKey
): Promise<[PublicKey, number]> {
  const [address, bump] = await PublicKey.findProgramAddress([AMM_CONFIG_SEED], programId);
  return [address, bump];
}

export async function getFeeAddress(
  fee: number,
  programId: PublicKey
): Promise<[PublicKey, number]> {
  const [address, bump] = await PublicKey.findProgramAddress(
    [FEE_SEED, u32ToBytes(fee)],
    programId
  );
  return [address, bump];
}

export async function getPoolAddress(
  ammConfig: PublicKey,
  tokenMint0: PublicKey,
  tokenMint1: PublicKey,
  programId: PublicKey,
  fee: number
): Promise<[PublicKey, number]> {
  const [address, bump] = await PublicKey.findProgramAddress(
    [
      POOL_SEED,
      ammConfig.toBuffer(),
      tokenMint0.toBuffer(),
      tokenMint1.toBuffer(),
      u32ToBytes(fee),
    ],
    programId
  );
  return [address, bump];
}

export async function getPoolVaultAddress(
  pool: PublicKey,
  vaultTokenMint: PublicKey,
  programId: PublicKey
): Promise<[PublicKey, number]> {
  const [address, bump] = await PublicKey.findProgramAddress(
    [POOL_VAULT_SEED, pool.toBuffer(), vaultTokenMint.toBuffer()],
    programId
  );
  return [address, bump];
}

export async function getPoolRewardVaultAddress(
  pool: PublicKey,
  rewardTokenMint: PublicKey,
  programId: PublicKey
): Promise<[PublicKey, number]> {
  const [address, bump] = await PublicKey.findProgramAddress(
    [POOL_REWARD_VAULT_SEED, pool.toBuffer(), rewardTokenMint.toBuffer()],
    programId
  );
  return [address, bump];
}


export async function getObservationAddress(
    pool: PublicKey,
    programId: PublicKey,
    index: number,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
        [OBSERVATION_SEED, pool.toBuffer(), u16ToBytes(index)],
      programId
    );
    return [address, bump];
  }

  export async function getTickAddress(
    pool: PublicKey,
    programId: PublicKey,
    tickIndex: number,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
        [TICK_SEED, pool.toBuffer(), i32ToBytes(tickIndex)],
      programId
    );
    return [address, bump];
  }

  export async function getTickBitmapAddress(
    pool: PublicKey,
    programId: PublicKey,
    word: number,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
        [BITMAP_SEED, pool.toBuffer(), i16ToBytes(word)],
      programId
    );
    return [address, bump];
  }

  export async function getProtocolPositionAddress(
    pool: PublicKey,
    programId: PublicKey,
    tickLower: number,
    tickUpper: number,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
      [
        POSITION_SEED,
        pool.toBuffer(),
        i32ToBytes(tickLower),
        i32ToBytes(tickUpper),
      ],
      programId
    );
    return [address, bump];
  }

  export async function getPersonalPositionAddress(
    nftMint: PublicKey,
    programId: PublicKey,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
      [
        POSITION_SEED,
        nftMint.toBuffer(),
      ],
      programId
    );
    return [address, bump];
  }

  export async function getNftMetadataAddress(
    nftMint: PublicKey,
  ): Promise<[PublicKey, number]> {
    const [address, bump] = await PublicKey.findProgramAddress(
        [
          Buffer.from("metadata"),
          metaplex.programs.metadata.MetadataProgram.PUBKEY.toBuffer(),
          nftMint.toBuffer(),
        ],
        metaplex.programs.metadata.MetadataProgram.PUBKEY
      )
      return [address, bump];
  }