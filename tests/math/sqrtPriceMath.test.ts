import {
  SqrtPriceMath,
  sqrtPriceX64ToPrice,
  priceToSqrtPriceX64
} from "./sqrtPriceMath";
import { assert, expect } from "chai";
import { BN } from "@project-serum/anchor";
import Decimal from "decimal.js";

describe("SqrtPriceMath test", async () => {
  describe("getSqrtPriceX64FromTick", () => {
    it("tick is overflow", async () => {
      SqrtPriceMath.getSqrtPriceX64FromTick(10);
    });
    it("get sqrt price from tick 10", async () => {
      assert.equal(
        SqrtPriceMath.getSqrtPriceX64FromTick(10).toString(),
        new BN("18455969290605287889").toString()
      );
    });
  });

  describe("getTickFromSqrtPriceX64", () => {
    it("get tick 10 from sqrt price", () => {
      assert.equal(
        SqrtPriceMath.getTickFromSqrtPriceX64(new BN("18455969290605287889")),
        10
      );
    });
  });

  describe("sqrtPriceX64ToPrice", () => {
    it("tick 10 from sqrt price to price", () => {
      assert.equal(
        sqrtPriceX64ToPrice(new BN("18455969290605287889")).toString(),
        "1.0010004501200207272"
      );
    });
  });

  describe("priceToSqrtX64", () => {
    it("get tick 10 from sqrt price", () => {
      assert.equal(
        priceToSqrtPriceX64(new Decimal("1.0010004501200207272")).toString(),
        new BN("18455969290605287889").toString()
      );
    });
  });
});
