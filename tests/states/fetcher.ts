import { Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { AmmCore } from "../../target/types/amm_core";
import {
  PoolState,
  TickState,
  PositionState,
  ObservationState,
  AmmConfig,
  PositionRewardInfo,
  RewardInfo,
} from "./states";

export class StateFetcher {
  program: Program<AmmCore>;

  constructor(program: Program<AmmCore>) {
    this.program = program;
  }

  public async getAmmConfig(address: PublicKey): Promise<AmmConfig> {
    const { bump, owner, protocolFeeRate } =
      await this.program.account.ammConfig.fetch(address);
    return {
      bump,
      owner,
      protocolFeeRate,
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
      tick,
      tickSpacing,
      liquidity,
      sqrtPriceX64,
      feeGrowthGlobal0,
      feeGrowthGlobal1,
      protocolFeesToken0,
      protocolFeesToken1,
      rewardLastUpdatedTimestamp,
      rewardInfos,
      observationIndex,
      observationCardinality,
      observationCardinalityNext,
    } = await this.program.account.poolState.fetch(address);

    // for (var i = 0; i < rewardInfos.len(); i++) {
    //     factorial *= i;
    // }
    return {
      bump,
      ammConfig,
      owner,
      tokenMint0,
      tokenMint1,
      tokenVault0,
      tokenVault1,
      tick,
      tickSpacing,
      liquidity,
      sqrtPriceX64,
      feeGrowthGlobal0,
      feeGrowthGlobal1,
      protocolFeesToken0,
      protocolFeesToken1,
      rewardLastUpdatedTimestamp,
      // rewardInfos,
      observationIndex,
      observationCardinality,
      observationCardinalityNext,
    };
  }

  public async getTickState(address: PublicKey): Promise<TickState> {
    const {
      bump,
      tick,
      liquidityNet,
      liquidityGross,
      feeGrowthOutside0X64,
      feeGrowthOutside1X64,
      tickCumulativeOutside,
      secondsPerLiquidityOutsideX64,
      secondsOutside,
      rewardGrowthsOutside,
    } = await this.program.account.tickState.fetch(address);

    return {
      bump,
      tick,
      liquidityNet,
      liquidityGross,
      feeGrowthOutside0X64,
      feeGrowthOutside1X64,
      tickCumulativeOutside,
      secondsPerLiquidityOutsideX64,
      secondsOutside,
      rewardGrowthsOutside,
    };
  }

  public async getPositionState(address: PublicKey): Promise<PositionState> {
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
      rewardInfos
    } = await this.program.account.personalPositionState.fetch(address);

    // if (rewardInfos instanceof PositionRewardInfo){

    // }

  console.log("getPositionState, rewardInfos:",rewardInfos)
    // const [] = rewardInfos[Symbol.iterator]()
    return {
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
      // rewardInfos,
    };
  }

  public async getObservationState(
    address: PublicKey
  ): Promise<ObservationState> {
    const {
      bump,
      index,
      blockTimestamp,
      tickCumulative,
      secondsPerLiquidityCumulativeX64,
      initialized,
    } = await this.program.account.observationState.fetch(address);

    return {
      bump,
      index,
      blockTimestamp,
      tickCumulative,
      secondsPerLiquidityCumulativeX64,
      initialized,
    };
  }
}
