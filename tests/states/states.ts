import { BN } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";

export type AmmConfig = {
  bump: number;
  owner: PublicKey;
  protocolFeeRate: number;
};

export type FeeState = {
  bump: number;
  fee: number;
  tickSpacing: number;
};

export type ObservationState = {
  bump: number;
  index: number;
  blockTimestamp: number;
  tickCumulative: BN;
  secondsPerLiquidityCumulativeX64: BN;
  initialized: boolean;
};

export type PoolState = {
  bump: number;
  ammConfig: PublicKey;
  owner: PublicKey;
  tokenMint0: PublicKey;
  tokenMint1: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  feeRate: number;
  tick: number;
  tickSpacing: number;
  liquidity: BN;
  sqrtPriceX64: BN;
  feeGrowthGlobal0: BN;
  feeGrowthGlobal1: BN;
  protocolFeesToken0: BN;
  protocolFeesToken1: BN;
  rewardLastUpdatedTimestamp: BN;
  // rewardInfos: RewardInfo[];
  observationIndex: number;
  observationCardinality: number;
  observationCardinalityNext: number;
};

export type RewardInfo = {
  rewardState: number;
  openTime: BN;
  endTime: BN;
  lastUpdateTime: BN;
  emissionsPerSecondX64: BN;
  rewardTotalEmissioned: BN;
  rewardClaimed: BN;
  tokenMint: PublicKey;
  tokenVault: PublicKey;
  authority: PublicKey;
  rewardGrowthGlobalX64: BN;
};

export type PositionState = {
  bump: number;
  nftMint: PublicKey;
  poolId: PublicKey;
  tickLower: number;
  tickUpper: number;
  liquidity: BN;
  // Q64.64
  feeGrowthInside0Last: BN;
  // Q64.64
  feeGrowthInside1Last: BN;
  tokenFeesOwed0: BN;
  tokenFeesOwed1: BN;
  // rewardInfos: PositionRewardInfo[];
};

export type PositionRewardInfo = {
  // Q64.64
  growthInsideLast: BN;
  rewardAmountOwed: BN;
};

export type TickState = {
  bump: number;
  tick: number;
  liquidityNet: BN;
  liquidityGross: BN;
  feeGrowthOutside0X64: BN;
  feeGrowthOutside1X64: BN;
  tickCumulativeOutside: BN;
  secondsPerLiquidityOutsideX64: BN;
  secondsOutside: number;
  rewardGrowthsOutside: BN[];
};
