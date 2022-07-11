
import {
    SwapMath,
    SqrtPriceMath
} from "."
import { FeeAmount } from '../entities/fee'

import JSBI from 'jsbi'


describe("amm-core", async () => {

    const feePips = FeeAmount.LOW
    let  [sqrtPriceX64, amountIn,amountOut, feeAmount] = SwapMath.computeSwapStep(
        JSBI.BigInt("18455969190605289472"),
        JSBI.BigInt("18446744073709551616"),
        JSBI.BigInt("1998600039"),
        JSBI.BigInt("1000000"),
        feePips,
        )
      console.log("step.amountIn:", amountIn.toString(),"step.amountOut", amountOut.toString())


     const ss =  SqrtPriceMath.getAmount0Delta(  JSBI.BigInt("18455969190605289472"),
      JSBI.BigInt("18446744073709551616"),
      JSBI.BigInt("1998600039"), true)

      console.log("ssss: ",ss.toString() )
})