import { BN } from "@project-serum/anchor"

/**
 * Decodes the 256 bit bitmap stored in a bitmap account
 * @param x Bitmap encoded as [u64; 4]
 * @returns 256 bit word
 */
export function generateBitmapWord(x: BN[]) {
  return x[0]
    .add(x[1].shln(64))
    .add(x[2].shln(128))
    .add(x[3].shln(192))
}

/**
 * Returns the most significant non-zero bit in the word
 * @param x
 * @returns
 */
export function msb(x: BN) {
  return x.bitLength() - 1
}

/**
 * Returns the least significant non-zero bit in the word
 * @param x
 * @returns
 */
export function lsb(x: BN) {
  return x.zeroBits()
}

export type NextBit = {
  next: number,
  initialized: boolean,
}

/**
 * Returns the bitmap index (0 - 255) for the next initialized tick.
 *
 * If no initialized tick is available, returns the first bit (index 0) the word in lte case,
 * and the last bit in gte case.
 * @param word The bitmap word as a u256 number
 * @param bitPos The starting bit position
 * @param lte Whether to search for the next initialized tick to the left (less than or equal to the starting tick),
 * or to the right (greater than or equal to)
 * @returns Bit index and whether it is initialized
 */
export function nextInitializedBit(word: BN, bitPos: number, lte: boolean): NextBit {
  if (lte) {
    // all the 1s at or to the right of the current bit_pos
    const mask = new BN(1).shln(bitPos).subn(1).add(new BN(1).shln(bitPos))
    const masked = word.and(mask)
    const initialized = !masked.eqn(0)
    const next = initialized
      ? msb(masked)
      : 0
    return { next, initialized }
  } else {
    // all the 1s at or to the left of the bit_pos
    const mask = new BN(1).shln(bitPos).subn(1).notn(256)
    const masked = word.and(mask)
    const initialized = !masked.eqn(0)
    const next = initialized
      ? lsb(masked)
      : 255
    return { next, initialized }
  }
}

export function buildTick(wordPos: number, nextBit: number, tickSpacing: number) {
  return ((wordPos << 8) + nextBit) * tickSpacing
}