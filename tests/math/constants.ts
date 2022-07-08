import { BN } from '@project-serum/anchor'
import JSBI from 'jsbi'

// constants used internally but not expected to be used externally
export const NEGATIVE_ONE = JSBI.BigInt(-1)
export const ZERO = JSBI.BigInt(0)
export const ONE = JSBI.BigInt(1)

// used in liquidity amount math
export const Q64 = JSBI.exponentiate(JSBI.BigInt(2), JSBI.BigInt(64))
export const BASE_X64_NUMBER = new BN(1).shln(64)
export const Q128 = JSBI.exponentiate(JSBI.BigInt(2), JSBI.BigInt(128))

// export const MaxUint32 = JSBI.subtract(Q32, ONE)
export const MaxUint64 = JSBI.subtract(Q64, ONE)
// export const MaxUint128 = JSBI.subtract(Q128, ONE)

export const U64Resolution = JSBI.BigInt(64)

export const MaxUint128 = JSBI.BigInt('0xffffffffffffffffffffffffffffffff')
