import {TickMath} from "./tickMath";
import { assert, expect } from "chai";
import JSBI from "jsbi";

describe("tickMath test", async()=>{

    describe("getSqrtPriceX64FromTick", () =>{
        it("tick is overflow", async()=>{
            TickMath.getSqrtPriceX64FromTick(10)
        })
        it("get sqrt price from tick 10", async()=>{
            assert.equal(TickMath.getSqrtPriceX64FromTick(10).toString(),JSBI.BigInt('18455969290605287889').toString())
        })
    });

    describe("getTickFromSqrtPriceX64", ()  =>{
        it("get tick 10 from sqrt price", () =>{
            assert.equal(TickMath.getTickFromSqrtPriceX64(JSBI.BigInt('18455969290605287889')), 10)
        })
    });
})