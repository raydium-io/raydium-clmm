import BN from "bn.js";
import * as anchor from "@project-serum/anchor";
import {getMultipleAccountsInfo} from "@raydium-io/raydium-sdk";
export const MIN_SQRT_RATIO = new BN(4295048016);
export const MAX_SQRT_RATIO = new BN("79226673521066979257578248091");

export const MIN_TICK = -443636;
export const MAX_TICK = 443636;

export const MaxU64 = new BN(2).pow(new BN(64)).subn(1);

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
