import BN from "bn.js"

export const MIN_SQRT_RATIO = new BN(65536)
export const MAX_SQRT_RATIO = new BN(281474976710656)

export const MIN_TICK = -221818
export const MAX_TICK = 221818

export const MaxU64 = new BN(2).pow(new BN(64)).subn(1)
