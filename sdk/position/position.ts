import { PublicKey, TokenAccountsFilter } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, MintLayout, AccountLayout } from "@solana/spl-token";
import {
  PersonalPositionState,
  RewardInfo,
  StateFetcher,
  TickState,
} from "../states";
import {
  getPersonalPositionAddress,
} from "../utils";
import { Context } from "../base";
import { MathUtil, Q64 } from "../math";
import { BN } from "@project-serum/anchor";
import { REWARD_NUM, AmmPool } from "../pool";
import { getTickArrayAddressByTick, getTickOffsetInArray } from "../entities";

export type MultiplePosition = {
  pubkey: PublicKey;
  state: PersonalPositionState;
};

export async function fetchAllPositionsByOwner(
  ctx: Context,
  owner: PublicKey,
  stateFetcher: StateFetcher
): Promise<
  {
    pubkey: PublicKey;
    state: PersonalPositionState;
  }[]
> {
  const filter: TokenAccountsFilter = { programId: TOKEN_PROGRAM_ID };
  const result = await ctx.connection.getTokenAccountsByOwner(owner, filter);

  let allPositions: {
    pubkey: PublicKey;
    state: PersonalPositionState;
  }[] = [];

  let allMints: PublicKey[] = [];
  for (let i = 0; i < result.value.length; i++) {
    const { mint } = AccountLayout.decode(result.value[i].account.data);
    // console.log(mint)
    allMints.push(new PublicKey(mint));
  }
  const fetchCount = Math.ceil(allMints.length / 100);

  for (let i = 0; i < fetchCount; i++) {
    const start = i * 100;
    let end = start + 100;
    if (end > allMints.length) {
      end = allMints.length;
    }
    const mints = allMints.slice(start, end);
    let positionAddresses: PublicKey[] = [];

    const mintAccountInfos = await ctx.connection.getMultipleAccountsInfo(
      mints
    );
    for (const [i, info] of mintAccountInfos.entries()) {
      if (info) {
        const { supply, decimals } = MintLayout.decode(info.data);
        const sup = supply.readBigInt64LE();
        // console.log(sup, supply, decimals)
        if (sup == 1 && decimals === 0) {
          const [positionAddress] = await getPersonalPositionAddress(
            mints[i],
            ctx.program.programId
          );
          positionAddresses.push(positionAddress);
        }
      }
    }

    const states = await stateFetcher.getMultiplePersonalPositionStates(
      positionAddresses
    );
    for (const [i, state] of states.entries()) {
      if (state) {
        allPositions.push({ pubkey: positionAddresses[i], state });
      }
    }
  }
  return allPositions;
}

export async function GetPositionRewardsWithFetchState(
  ctx: Context,
  poolId: PublicKey,
  positionId: PublicKey,
  stateFetcher: StateFetcher
): Promise<number[]> {
  const ammPool = new AmmPool(ctx, poolId, stateFetcher);
  await ammPool.loadPoolState();

  const personalPositionData = await stateFetcher.getPersonalPositionState(
    new PublicKey(positionId)
  );

  const tickArrayLowerAddress = await getTickArrayAddressByTick(
    ctx.program.programId,
    ammPool.address,
    personalPositionData.tickLowerIndex,
    ammPool.poolState.tickSpacing
  );
  const tickArrayUpperAddress = await getTickArrayAddressByTick(
    ctx.program.programId,
    ammPool.address,
    personalPositionData.tickUpperIndex,
    ammPool.poolState.tickSpacing
  );
  const tickArrayStates = await stateFetcher.getMultipleTickArrayState([
    tickArrayLowerAddress,
    tickArrayUpperAddress,
  ]);
  const tickLowerState =
    tickArrayStates[0].ticks[
      getTickOffsetInArray(
        personalPositionData.tickLowerIndex,
        ammPool.poolState.tickSpacing
      )
    ];
  const tickUpperState =
    tickArrayStates[1].ticks[
      getTickOffsetInArray(
        personalPositionData.tickUpperIndex,
        ammPool.poolState.tickSpacing
      )
    ];
  // console.log("tickLowerState:", tickLowerState);
  // console.log("tickUpperState:", tickUpperState);
  return GetPositionRewards(
    ammPool,
    personalPositionData,
    tickLowerState,
    tickUpperState
  );
}

export async function GetPositionRewards(
  ammPool: AmmPool,
  positionState: PersonalPositionState,
  tickLowerState: TickState,
  tickUpperState: TickState
): Promise<number[]> {
  let rewards: number[] = [];

  const updatedRewardInfos = await ammPool.simulate_update_rewards();

  // console.log("updatedRewardInfos:", updatedRewardInfos);
  const rewardGrowthsInside = getRewardGrowthInside(
    ammPool.poolState.tickCurrent,
    tickLowerState,
    tickUpperState,
    updatedRewardInfos
  );
  // console.log("rewardGrowthsInside:", rewardGrowthsInside);
  for (let i = 0; i < REWARD_NUM; i++) {
    let rewardGrowthInside = rewardGrowthsInside[i];
    let currRewardInfo = positionState.rewardInfos[i];

    let rewardGrowthDelta = rewardGrowthInside.sub(
      currRewardInfo.growthInsideLastX64
    );

    let amountOwedDelta = MathUtil.mulDivFloor(
      rewardGrowthDelta,
      positionState.liquidity,
      Q64
    );
    const rewardAmountOwed =
      currRewardInfo.rewardAmountOwed.add(amountOwedDelta);
    rewards.push(rewardAmountOwed.toNumber());
  }
  return rewards;
}

function getRewardGrowthInside(
  tickCurrentIndex: number,
  tickLowerState: TickState,
  tickUpperState: TickState,
  rewardInfos: RewardInfo[]
): BN[] {
  let rewardGrowthsInside: BN[] = [];
  for (let i = 0; i < REWARD_NUM; i++) {
    if (rewardInfos[i].tokenMint.equals(PublicKey.default)) {
      rewardGrowthsInside.push(new BN(0));
      continue;
    }

    // By convention, assume all prior growth happened below the tick
    let rewardGrowthsBelow = new BN(0);
    if (tickLowerState.liquidityGross.eqn(0)) {
      rewardGrowthsBelow = rewardInfos[i].rewardGrowthGlobalX64;
    } else if (tickCurrentIndex < tickLowerState.tick) {
      rewardGrowthsBelow = rewardInfos[i].rewardGrowthGlobalX64.sub(
        tickLowerState.rewardGrowthsOutsideX64[i]
      );
    } else {
      rewardGrowthsBelow = tickLowerState.rewardGrowthsOutsideX64[i];
    }

    // By convention, assume all prior growth happened below the tick, not above
    let rewardGrowthsAbove = new BN(0);
    if (tickUpperState.liquidityGross.eqn(0)) {
    } else if (tickCurrentIndex < tickUpperState.tick) {
      rewardGrowthsAbove = tickUpperState.rewardGrowthsOutsideX64[i];
    } else {
      rewardGrowthsAbove = rewardInfos[i].rewardGrowthGlobalX64.add(
        tickUpperState.rewardGrowthsOutsideX64[i]
      );
    }

    rewardGrowthsInside.push(
      rewardInfos[i].rewardGrowthGlobalX64
        .sub(rewardGrowthsBelow)
        .sub(rewardGrowthsAbove)
    );
  }

  return rewardGrowthsInside;
}
