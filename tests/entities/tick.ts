

export type TickPosition = {
    wordPos: number
    bitPos: number
  }
  
  /**
   *  Computes the bitmap position for a bit.
   * @param tick 
   * @param tickSpacing 
   * @returns the word and bit position for the given tick
   */
  export function tickPosition(tick: number, tickSpacing: number): TickPosition {
   const  tickBySpacing = tick / tickSpacing
    return {
      wordPos: tickBySpacing >> 8,
      bitPos: tickBySpacing % 256 & 255 // mask with 255 to get the output
    }
  }