import { Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { AmmV3 } from "../anchor/amm_v3";
import {
  PoolState,
  TickState,
  PositionState,
  ObservationState,
  Observation,
  AmmConfig,
  PositionRewardInfo,
  RewardInfo,
  TickArrayState,
} from "./states";

export class StateFetcher {
  private program: Program<AmmV3>;

  constructor(program: Program<AmmV3>) {
    this.program = program;
  }

  public async getAmmConfig(address: PublicKey): Promise<AmmConfig> {
    const { bump, index, owner, protocolFeeRate, tradeFeeRate, tickSpacing } =
      await this.program.account.ammConfig.fetch(address);
    return {
      bump,
      index,
      owner,
      protocolFeeRate,
      tradeFeeRate,
      tickSpacing,
    };
  }

  public async getPoolState(address: PublicKey): Promise<PoolState> {
    const {
      bump,
      ammConfig,
      owner,
      tokenMint0,
      tokenMint1,
      tokenVault0,
      tokenVault1,
      tickCurrent,
      tickSpacing,
      liquidity,
      sqrtPriceX64,
      feeGrowthGlobal0X64,
      feeGrowthGlobal1X64,
      protocolFeesToken0,
      protocolFeesToken1,
      rewardLastUpdatedTimestamp,
      rewardInfos,
      observationIndex,
      observationKey,
      observationUpdateDuration,
    } = await this.program.account.poolState.fetch(address);

    return {
      bump,
      ammConfig,
      owner,
      tokenMint0,
      tokenMint1,
      tokenVault0,
      tokenVault1,
      tickCurrent,
      tickSpacing,
      liquidity,
      sqrtPriceX64,
      feeGrowthGlobal0X64,
      feeGrowthGlobal1X64,
      protocolFeesToken0,
      protocolFeesToken1,
      rewardLastUpdatedTimestamp,
      // rewardInfos,
      observationIndex,
      observationKey,
      observationUpdateDuration,
    };
  }

  public async getTickArrayState(address: PublicKey): Promise<TickArrayState> {
    const { ammPool, startTickIndex, ticks } =
      await this.program.account.tickArrayState.fetch(address);

    // bump,
    // tick,
    // liquidityNet,
    // liquidityGross,
    // feeGrowthOutside0X64,
    // feeGrowthOutside1X64,
    // tickCumulativeOutside,
    // secondsPerLiquidityOutsideX64,
    // secondsOutside,
    // rewardGrowthsOutside,

    const tickStates = ticks as TickState[];
    return {
      ammPool,
      startTickIndex,
      ticks: tickStates,
    };
  }

  public async getPersonalPositionState(
    address: PublicKey
  ): Promise<PositionState> {
    const {
      bump,
      nftMint,
      poolId,
      tickLowerIndex,
      tickUpperIndex,
      liquidity,
      feeGrowthInside0Last,
      feeGrowthInside1Last,
      tokenFeesOwed0,
      tokenFeesOwed1,
      rewardInfos,
    } = await this.program.account.personalPositionState.fetch(address);

    // if (rewardInfos instanceof PositionRewardInfo){

    // }
    // const [] = rewardInfos[Symbol.iterator]()
    return {
      bump,
      nftMint,
      poolId,
      tickLowerIndex,
      tickUpperIndex,
      liquidity,
      feeGrowthInside0LastX64: feeGrowthInside0Last,
      feeGrowthInside1LastX64: feeGrowthInside1Last,
      tokenFeesOwed0,
      tokenFeesOwed1,
      // rewardInfos,
    };
  }

  public async getObservationState(
    address: PublicKey
  ): Promise<ObservationState> {
    const { initialized, observations } =
      await this.program.account.observationState.fetch(address);

    return {
      initialized,
      observations: observations as Observation[],
    };
  }
}
