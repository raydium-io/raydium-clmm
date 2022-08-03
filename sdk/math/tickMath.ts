import Decimal from "decimal.js";
import { SqrtPriceMath } from "./sqrtPriceMath";

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
