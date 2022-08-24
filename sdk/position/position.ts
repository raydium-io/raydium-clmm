import { PublicKey, TokenAccountsFilter } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID,MintLayout, AccountLayout } from "@solana/spl-token";
import { PositionState, StateFetcher } from "../states";
import { getPersonalPositionAddress } from "../utils";
import { Context } from "../base";

export type MultiplePosition = {
  pubkey: PublicKey;
  state: PositionState;
};

export async function fetchAllPositionsByOwner(
  ctx: Context,
  owner: PublicKey,
  stateFetcher: StateFetcher
): Promise<
  {
    pubkey: PublicKey;
    state: PositionState;
  }[]
> {
  const filter: TokenAccountsFilter = { programId: TOKEN_PROGRAM_ID };
  const result = await ctx.connection.getTokenAccountsByOwner(owner, filter);

  let allPositions: {
    pubkey: PublicKey;
    state: PositionState;
  }[] = [];

  let allMints: PublicKey[] = [];
  for (let i = 0; i < result.value.length; i++) {
    const {mint} = AccountLayout.decode(result.value[i].account.data);
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

    const mintAccountInfos = await ctx.connection.getMultipleAccountsInfo(mints);
    for (const [i,info] of mintAccountInfos.entries()) {
      if (info) {
        const { supply, decimals } = MintLayout.decode(info.data);
        const sup = supply.readBigInt64LE()
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

    const states = await stateFetcher.getMultiplePersonalPositionStates(positionAddresses);
    for (const [i,state] of states.entries()) {
      if (state) {
        allPositions.push({ pubkey: positionAddresses[i], state });
      }
    }
  }
  return allPositions;
}
