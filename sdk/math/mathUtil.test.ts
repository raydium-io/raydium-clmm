import { MathUtil, SqrtPriceMath, LiquidityMath } from ".";
import { assert } from "chai";
import { BN } from "@project-serum/anchor";
import Decimal from "decimal.js";

describe("math test", async () => {
  describe("liquidityMath test", async () => {
    const currentSqrtPriceX64 = new BN("226137466252783162");
    const sqrtPriceX64A = new BN("189217263716712836");
    const sqrtPriceX64B = new BN("278218361723627362");
    it("getLiquidityFromTokenAmounts", () => {
      let amount0 = new BN(102912736398);
      let amount1 = new BN(1735327364384);
      let liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
        currentSqrtPriceX64,
        sqrtPriceX64A,
        sqrtPriceX64B,
        amount0,
        amount1
      );
      console.log(
        "getLiquidityFromTokenAmounts liquidity:",
        liquidity.toString(),
        "amount0:",
        amount0.toString(),
        "amount1:",
        amount1.toString()
      );
      [amount0, amount1] = LiquidityMath.getAmountsFromLiquidity(
        currentSqrtPriceX64,
        sqrtPriceX64A,
        sqrtPriceX64B,
        liquidity,
        true
      );
      console.log(
        "getAmountsFromLiquidity      liquidity:",
        liquidity.toString(),
        "amount0:",
        amount0.toString(),
        "amount1:",
        amount1.toString()
      );

      amount0 = new BN(47367378458);
      amount1 = new BN(2395478753487);
      liquidity = LiquidityMath.getLiquidityFromTokenAmounts(
        currentSqrtPriceX64,
        sqrtPriceX64A,
        sqrtPriceX64B,
        amount0,
        amount1
      );
      console.log(
        "getLiquidityFromTokenAmounts liquidity:",
        liquidity.toString(),
        "amount0:",
        amount0.toString(),
        "amount1:",
        amount1.toString()
      );
      [amount0, amount1] = LiquidityMath.getAmountsFromLiquidity(
        currentSqrtPriceX64,
        sqrtPriceX64A,
        sqrtPriceX64B,
        liquidity,
        true
      );
      console.log(
        "getAmountsFromLiquidity      liquidity:",
        liquidity.toString(),
        "amount0:",
        amount0.toString(),
        "amount1:",
        amount1.toString()
      );
    });
  });

  it("mulDivRoundingUp", () => {
    assert.equal(
      MathUtil.mulDivRoundingUp(new BN(2), new BN(5), new BN(3)).toNumber(),
      new BN(4).toNumber()
    );
    assert.equal(
      MathUtil.mulDivRoundingUp(new BN(2), new BN(2), new BN(2)).toNumber(),
      new BN(2).toNumber()
    );
  });

  it("mulDivFloor", () => {
    assert.equal(
      MathUtil.mulDivFloor(new BN(2), new BN(2), new BN(3)).toNumber(),
      new BN(1).toNumber()
    );
  });

  it("mulDivCeil", () => {
    assert.equal(
      MathUtil.mulDivCeil(new BN(2), new BN(5), new BN(3)).toNumber(),
      new BN(4).toNumber()
    );
    assert.equal(
      MathUtil.mulDivCeil(new BN(2), new BN(2), new BN(3)).toNumber(),
      new BN(2).toNumber()
    );
  });

  it("x64ToDecimal", () => {
    const sqrtPriceX64_5 = SqrtPriceMath.getSqrtPriceX64FromTick(5);
    assert.equal(
      MathUtil.x64ToDecimal(sqrtPriceX64_5, 16).toString(), // 1.000250018750312252
      new Decimal("1.0002500187503123").toString()
    );
    const sqrtPriceX64_6 = SqrtPriceMath.getSqrtPriceX64FromTick(6);
    assert.equal(
      MathUtil.x64ToDecimal(sqrtPriceX64_6, 16).toString(), // 1.0003000300009999222
      new Decimal("1.0003000300009999").toString()
    );

    const sqrtPriceX64_neg_5 = SqrtPriceMath.getSqrtPriceX64FromTick(-5);
    assert.equal(
      MathUtil.x64ToDecimal(sqrtPriceX64_neg_5, 16).toString(), // 0.99975004374343864617
      new Decimal("0.9997500437434386").toString()
    );
    const sqrtPriceX64_neg_6 = SqrtPriceMath.getSqrtPriceX64FromTick(-6);
    assert.equal(
      MathUtil.x64ToDecimal(sqrtPriceX64_neg_6, 16).toString(), // 0.99970005999000157755
      new Decimal("0.9997000599900016").toString()
    );
  });

  it("decimalToX64", () => {
    const sqrtPriceX64_5 = SqrtPriceMath.getSqrtPriceX64FromTick(5);
    assert.equal(
      MathUtil.decimalToX64(new Decimal("1.000250018750312252")).toString(),
      sqrtPriceX64_5.toString()
    );
    const sqrtPriceX64_6 = SqrtPriceMath.getSqrtPriceX64FromTick(6);
    assert.equal(
      MathUtil.decimalToX64(new Decimal("1.0003000300009999222"))
        .subn(1)
        .toString(),
      sqrtPriceX64_6.toString()
    );
    const sqrtPriceX64_neg_5 = SqrtPriceMath.getSqrtPriceX64FromTick(-5);
    assert.equal(
      MathUtil.decimalToX64(new Decimal("0.99975004374343864617")).toString(),
      sqrtPriceX64_neg_5.toString()
    );
    const sqrtPriceX64_neg_6 = SqrtPriceMath.getSqrtPriceX64FromTick(-6);
    assert.equal(
      MathUtil.decimalToX64(new Decimal("0.99970005999000157755")).toString(),
      sqrtPriceX64_neg_6.toString()
    );
  });
});
