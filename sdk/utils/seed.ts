import * as anchor from "@project-serum/anchor";

export const AMM_CONFIG_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("amm_config"));
export const POOL_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("pool"));
export const POOL_VAULT_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("pool_vault")
);
export const POOL_REWARD_VAULT_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("pool_reward_vault")
);
export const POSITION_SEED = Buffer.from(
  anchor.utils.bytes.utf8.encode("position")
);
export const TICK_ARRAY_SEED = Buffer.from(anchor.utils.bytes.utf8.encode("tick_array"));

export function u16ToBytes(num: number) {
    const arr = new ArrayBuffer(2)
    const view = new DataView(arr)
    view.setUint16(0, num, false)
    return new Uint8Array(arr)
}

export function i16ToBytes(num: number) {
    const arr = new ArrayBuffer(2)
    const view = new DataView(arr)
    view.setInt16(0, num, false)
    return new Uint8Array(arr)
}

export function u32ToBytes(num: number) {
    const arr = new ArrayBuffer(4)
    const view = new DataView(arr)
    view.setUint32(0, num, false)
    return new Uint8Array(arr)
}

export function i32ToBytes(num: number) {
    const arr = new ArrayBuffer(4)
    const view = new DataView(arr)
    view.setInt32(0, num, false)
    return new Uint8Array(arr)
}