import { PublicKey } from "@solana/web3.js";
import { AmmConfig, StateFetcher } from "../states";

export class AmmConfigCache{
  static configs: Map<PublicKey, AmmConfig> = new Map()

  static async getConfig(stateFetcher: StateFetcher, key:PublicKey){
    const ret = AmmConfigCache.configs.get(key)
    if (ret) return ret

    const ammConfigData = await stateFetcher.getAmmConfig(key);
    if (ammConfigData){
        AmmConfigCache.configs.set(key, ammConfigData)
    }
    return ammConfigData

  }
}
