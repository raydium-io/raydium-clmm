import Decimal from "decimal.js";
import { SqrtPriceMath } from "./sqrtPriceMath";
import {TICK_ARRAY_SIZE} from "../entities"
export function getTickWithPriceAndTickspacing(
  price: Decimal,
  tickSpacing: number
) {
  const tick = SqrtPriceMath.getTickFromSqrtPriceX64(
    SqrtPriceMath.priceToSqrtPriceX64(price)
  );
  let result = tick / tickSpacing;
  if (result < 0) {
    result = Math.floor(result);
  } else {
    result = Math.ceil(result);
  }
  return result * tickSpacing;
}

export function roundPriceWithTickspacing(price: Decimal, tickSpacing: number) {
  const tick = getTickWithPriceAndTickspacing(price, tickSpacing);
  const sqrtPriceX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tick);
  return SqrtPriceMath.sqrtPriceX64ToPrice(sqrtPriceX64);
}

// export function getNextInitializedBit(tickArrayStartIndex: number,tickSpacing:number){
//   let tickArrayOffset = Math.floor(
//     tickArrayStartIndex / (tickSpacing * TICK_ARRAY_SIZE)
//   );
//   if (tickArrayStartIndex < 0) {
//     tickArrayOffset = Math.imul(tickArrayOffset, -1) - 1;
//   }

//   const n = Math.ceil(tickArrayOffset / 64);
//   let m = Math.floor(tickArrayOffset % 64);
//   m = 64 -m
//   for (let i = m - 1; i > 0; i--) {
//     if (
//       bitwise.integer
//         .getBit(tickArrayBitmapPositive[n].toNumber(), i)
//         .valueOf() == 1
//     ) {
//       const nextStartIndex =
//         ((n - 1) * 64 + i) * (tickSpacing * TICK_ARRAY_SIZE);
//       const [tickArrayAddress, _] = await getTickArrayAddress(
//         this.poolAddress,
//         this.program.programId,
//         nextStartIndex
//       );
//       tickArraysToFetch.push(tickArrayAddress);
//     }
//   }
// }