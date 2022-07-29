import { BN } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";

export const OBSERVATION_STATE_LEN = 52121;

export type AmmConfig = {
  bump: number;
  index: number;
  owner: PublicKey;
  protocolFeeRate: number;
  tradeFeeRate: number;
  tickSpacing: number;
};

export type ObservationState = {
  initialized: boolean;
  observations: Observation[];
};

export type Observation = {
  blockTimestamp: number;
  sqrtPriceX64: BN;
  tickCumulative: BN;
  cumulativeTimePriceX64: BN;
};

export type PoolState = {
  bump: number;
  ammConfig: PublicKey;
  owner: PublicKey;
  tokenMint0: PublicKey;
  tokenMint1: PublicKey;
  tokenVault0: PublicKey;
  tokenVault1: PublicKey;
  tickCurrent: number;
  tickSpacing: number;
  liquidity: BN;
  sqrtPriceX64: BN;
  feeGrowthGlobal0X64: BN;
  feeGrowthGlobal1X64: BN;
  protocolFeesToken0: BN;
  protocolFeesToken1: BN;
  rewardLastUpdatedTimestamp: BN;
  // rewardInfos: RewardInfo[];
  observationIndex: number;
  observationKey: PublicKey;
  observationUpdateDuration: number;
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
  tickLowerIndex: number;
  tickUpperIndex: number;
  liquidity: BN;
  // Q64.64
  feeGrowthInside0LastX64: BN;
  // Q64.64
  feeGrowthInside1LastX64: BN;
  tokenFeesOwed0: BN;
  tokenFeesOwed1: BN;
  // rewardInfos: PositionRewardInfo[];
};

export type PositionRewardInfo = {
  // Q64.64
  growthInsideLastX64: BN;
  rewardAmountOwed: BN;
};

export type TickArrayState = {
  ammPool: PublicKey;
  startTickIndex: number;
  ticks: TickState[];
};

export type TickState = {
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
