import {
  getInitializedTickArrayInRange,
  mergeTickArrayBitmap,
  checkTickArrayIsInitialized,
} from "./tickArray";

import { assert, expect } from "chai";
import { BN } from "@project-serum/anchor";

describe("tick array test", async () => {
  it("getInitializedTickArrayInRange", () => {
    let bns: BN[] = [
      new BN("1"), // -409600
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("9223372036854775808"), // -52000
      new BN("16140901064495857665"), // -800, -1600, -2400, -51200
      new BN("7"), // 0, 800, 1600
      new BN("1"), // 51200
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("9223372036854775808"), // 408800
    ];
    let bitmap = mergeTickArrayBitmap(bns);
    assert.deepEqual(
      getInitializedTickArrayInRange(bitmap, 10, 0, 7),
      [-800, -1600, -2400, -51200, -52000, -409600, 0, 800, 1600, 51200, 408800]
    );
  });

  it("getInitializedTickArrayInRange", () => {
    let bns: BN[] = [
      new BN("1"), // -409600
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("9223372036854775808"), // -52000
      new BN("16140901064495857665"), // -800, -1600, -2400, -51200
      new BN("7"), // 0, 800, 1600
      new BN("1"), // 51200
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("0"),
      new BN("9223372036854775808"), // 408800
    ];
    let bitmap = mergeTickArrayBitmap(bns);
    let [isInitialized, startIndex] = checkTickArrayIsInitialized(
      bitmap,
      0,
      10
    );
    assert.equal(isInitialized, true);
    [isInitialized, startIndex] = checkTickArrayIsInitialized(bitmap, -800, 10);
    assert.equal(isInitialized, true);
    assert.equal(startIndex, -800);
    [isInitialized, startIndex] = checkTickArrayIsInitialized(bitmap, -20, 10);
    assert.equal(isInitialized, true);
    assert.equal(startIndex, -800);
    [isInitialized, startIndex] = checkTickArrayIsInitialized(bitmap, 20, 10);
    assert.equal(isInitialized, true);
    assert.equal(startIndex, 0);
    [isInitialized, startIndex] = checkTickArrayIsInitialized(bitmap, 800, 10);
    assert.equal(isInitialized, true);
    assert.equal(startIndex, 800);
  });
});
