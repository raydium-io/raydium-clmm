import { Math } from "../math";

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
    assert.equal(Math.mulDivFloor(new BN(2), new BN(2), new BN(3)).toNumber(), new BN(1).toNumber());
  });

  it("mulDivCeil", () => {
    assert.equal(Math.mulDivCeil(new BN(2), new BN(5), new BN(3)).toNumber(), new BN(4).toNumber());
    assert.equal(Math.mulDivCeil(new BN(2), new BN(2), new BN(3)).toNumber(), new BN(2).toNumber());
  });

  it("x64ToDecimal", () => {
    assert.equal(
      Math.x64ToDecimal(new BN("18455969290605287889")).toString(),
      new Decimal("1.0005001000100003624").toString()
    );
  });

  it("decimalToX64", () => {
    assert.equal(
        Math.decimalToX64(  new Decimal("1.0005001000100003624")).toString(),
        new BN("18455969290605287889").toString()
      );
  });
});
