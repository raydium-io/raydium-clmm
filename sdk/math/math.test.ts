import { Math, SqrtPriceMath } from "../math";

import { assert, expect } from "chai";
import { BN } from "@project-serum/anchor";
import Decimal from "decimal.js";

describe("math test", async () => {
  it("mulDivRoundingUp", () => {
    assert.equal(
      Math.mulDivRoundingUp(new BN(2), new BN(5), new BN(3)).toNumber(),
      new BN(4).toNumber()
    );
    assert.equal(
      Math.mulDivRoundingUp(new BN(2), new BN(2), new BN(2)).toNumber(),
      new BN(2).toNumber()
    );
  });

  it("mulDivFloor", () => {
    assert.equal(
      Math.mulDivFloor(new BN(2), new BN(2), new BN(3)).toNumber(),
      new BN(1).toNumber()
    );
  });

  it("mulDivCeil", () => {
    assert.equal(
      Math.mulDivCeil(new BN(2), new BN(5), new BN(3)).toNumber(),
      new BN(4).toNumber()
    );
    assert.equal(
      Math.mulDivCeil(new BN(2), new BN(2), new BN(3)).toNumber(),
      new BN(2).toNumber()
    );
  });

  it("x64ToDecimal", () => {
    const sqrtPriceX64_5 = SqrtPriceMath.getSqrtPriceX64FromTick(5);
    assert.equal(
      Math.x64ToDecimal(sqrtPriceX64_5, 16).toString(), // 1.000250018750312252
      new Decimal("1.0002500187503123").toString()
    );
    const sqrtPriceX64_6 = SqrtPriceMath.getSqrtPriceX64FromTick(6);
    assert.equal(
      Math.x64ToDecimal(sqrtPriceX64_6, 16).toString(), // 1.0003000300009999222
      new Decimal("1.0003000300009999").toString()
    );

    const sqrtPriceX64_neg_5 = SqrtPriceMath.getSqrtPriceX64FromTick(-5);
    assert.equal(
      Math.x64ToDecimal(sqrtPriceX64_neg_5, 16).toString(), // 0.99975004374343864617
      new Decimal("0.9997500437434386").toString()
    );
    const sqrtPriceX64_neg_6 = SqrtPriceMath.getSqrtPriceX64FromTick(-6);
    assert.equal(
      Math.x64ToDecimal(sqrtPriceX64_neg_6, 16).toString(), // 0.99970005999000157755
      new Decimal("0.9997000599900016").toString()
    );
  });

  it("decimalToX64", () => {
    const sqrtPriceX64_5 = SqrtPriceMath.getSqrtPriceX64FromTick(5);
    assert.equal(
      Math.decimalToX64(new Decimal("1.000250018750312252")).toString(),
      sqrtPriceX64_5.toString()
    );
    const sqrtPriceX64_6 = SqrtPriceMath.getSqrtPriceX64FromTick(6);
    assert.equal(
      Math.decimalToX64(new Decimal("1.0003000300009999222"))
        .subn(1)
        .toString(),
      sqrtPriceX64_6.toString()
    );
    const sqrtPriceX64_neg_5 = SqrtPriceMath.getSqrtPriceX64FromTick(-5);
    assert.equal(
      Math.decimalToX64(new Decimal("0.99975004374343864617")).toString(),
      sqrtPriceX64_neg_5.toString()
    );
    const sqrtPriceX64_neg_6 = SqrtPriceMath.getSqrtPriceX64FromTick(-6);
    assert.equal(
      Math.decimalToX64(new Decimal("0.99970005999000157755")).toString(),
      sqrtPriceX64_neg_6.toString()
    );
  });
});
