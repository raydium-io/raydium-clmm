import Decimal from "decimal.js";
import { SqrtPriceMath } from "./sqrtPriceMath";

export function getTickWithPriceAndTickspacing(
  price: Decimal,
  tickSpacing: number,
  tokenMint0Decimals: number,
  tokenMint1Decimals: number
) {
  const tick = SqrtPriceMath.getTickFromSqrtPriceX64(
    SqrtPriceMath.priceToSqrtPriceX64(
      price,
      tokenMint0Decimals,
      tokenMint1Decimals
    )
  );
  let result = tick / tickSpacing;
  if (result < 0) {
    result = Math.floor(result);
  } else {
    result = Math.ceil(result);
  }
  return result * tickSpacing;
}

export function roundPriceWithTickspacing(
  price: Decimal,
  tickSpacing: number,
  tokenMint0Decimals: number,
  tokenMint1Decimals: number
) {
  const tick = getTickWithPriceAndTickspacing(
    price,
    tickSpacing,
    tokenMint0Decimals,
    tokenMint1Decimals
  );
  const sqrtPriceX64 = SqrtPriceMath.getSqrtPriceX64FromTick(tick);
  return SqrtPriceMath.sqrtPriceX64ToPrice(
    sqrtPriceX64,
    tokenMint0Decimals,
    tokenMint1Decimals
  );
}
