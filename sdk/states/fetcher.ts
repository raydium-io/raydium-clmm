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
    return await this.program.account.poolState.fetch(address) as PoolState;
  }

  public async getTickArrayState(address: PublicKey): Promise<TickArrayState> {
    const { ammPool, startTickIndex, ticks, initializedTickCount } =
      await this.program.account.tickArrayState.fetch(address);

    const tickStates = ticks as TickState[];
    return {
      ammPool,
      startTickIndex,
      ticks: tickStates,
      initializedTickCount,
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
      feeGrowthInside0LastX64,
      feeGrowthInside1LastX64,
      tokenFeesOwed0,
      tokenFeesOwed1,
      rewardInfos,
    } = await this.program.account.personalPositionState.fetch(address);

    const rewards = rewardInfos as PositionRewardInfo[];

    return {
      bump,
      nftMint,
      poolId,
      tickLowerIndex,
      tickUpperIndex,
      liquidity,
      feeGrowthInside0LastX64,
      feeGrowthInside1LastX64,
      tokenFeesOwed0,
      tokenFeesOwed1,
      rewardInfos: rewards,
    };
  }

  public async getMultiplePersonalPositionStates(
    addresses: PublicKey[]
  ): Promise<PositionState[]> {
    const result =
      await this.program.account.personalPositionState.fetchMultiple(addresses);
    return result as PositionState[];
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
