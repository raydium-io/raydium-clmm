import BN from "bn.js";
import * as anchor from "@project-serum/anchor";
import {getMultipleAccountsInfo} from "@raydium-io/raydium-sdk";

export const MIN_SQRT_RATIO = new BN(65536);
export const MAX_SQRT_RATIO = new BN(281474976710656);

export const MIN_TICK = -221818;
export const MAX_TICK = 221818;

export const MaxU64 = new BN(2).pow(new BN(64)).subn(1);

export const POOL_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("pool"));
export const POOL_VAULT_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("pool_vault")
);
export const FEE_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("fee"));
export const BITMAP_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("tick_bitmap")
);
export const POSITION_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("position")
);
export const TICK_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("tick"));
export const OBSERVATION_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("observation")
);


export async function accountExist(connection: anchor.web3.Connection, account: anchor.web3.PublicKey) {
  let alreadCreatedMarket = false
  let multipleInfo = await getMultipleAccountsInfo(connection, [account])
  if (multipleInfo.length > 0 && multipleInfo[0] !== null) {
      if (multipleInfo[0]?.data.length !== 0) {
          alreadCreatedMarket = true
      }
  }
  return alreadCreatedMarket;
}