import { BN } from "@project-serum/anchor";

// constants used internally but not expected to be used externally
export const ZERO = new BN(0);
export const ONE = new BN(1);
export const NEGATIVE_ONE = new BN(-1);

// used in liquidity amount math
export const Q64 = new BN(1).shln(64);
export const Q128 = new BN(1).shln(128);

// export const MaxUint32 = JSBI.subtract(Q32, ONE)
export const MaxU64 = Q64.sub(ONE);
// export const MaxUint128 = JSBI.subtract(Q128, ONE)

export const U64Resolution = 64;

export const MaxUint128 = Q128.subn(1);

/**
 * The minimum tick that can be used on any pool.
 */
export const MIN_TICK: number = -307200;
/**
 * The maximum tick that can be used on any pool.
 */
export const MAX_TICK: number = -MIN_TICK;

export const MIN_SQRT_PRICE_X64: BN = new BN("3939943522091");
export const MAX_SQRT_PRICE_X64: BN = new BN("86367321006760116002434269");

export const MIN_TICK_ARRAY_START_INDEX = -307200;
export const MAX_TICK_ARRAY_START_INDEX = 306600;
