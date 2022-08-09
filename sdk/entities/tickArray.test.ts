import {
  getInitializedTickArrayInRange,
  mergeTickArrayBitmap,
} from "./tickArray";

import { assert, expect } from "chai";
import { BN } from "@project-serum/anchor";

describe("tick array test", async () => {
  it("getInitializedTickArrayInRange", () => {
    let bns: BN[] = [
      new BN("1"),
      new BN("2"),
      new BN("3"),
      new BN("4"),
      new BN("5"),
      new BN("6"),
      new BN("7"),
      new BN("8"),
      new BN("1"),
      new BN("2"),
      new BN("3"),
      new BN("4"),
      new BN("5"),
      new BN("6"),
      new BN("7"),
      new BN("8"),
    ];
    let bitmap = mergeTickArrayBitmap(bns);
    assert.equal(getInitializedTickArrayInRange(bitmap, 10, 0, 7), []);
  });
});
