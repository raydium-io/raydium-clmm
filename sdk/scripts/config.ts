import { BN } from "@project-serum/anchor";
import { ConfirmOptions, PublicKey } from "@solana/web3.js";
import Decimal from "decimal.js";

export const defaultConfirmOptions: ConfirmOptions = {
  preflightCommitment: "processed",
  commitment: "processed",
  skipPreflight: true,
};

export const Config = {
  url: "https://api.devnet.solana.com",
  // url: "http://127.0.0.1:8899",
  programId: new PublicKey("DevadyVYwyiMQikvjkFYmiaobLNaGsJJbgsEL1Rfp3zK"),
  "create-amm-config": [
    {
      index: 0,
      tickSpacing: 10,
      tradeFeeRate: 100,
      protocolFeeRate: 12000,
    },
    {
      index: 1,
      tickSpacing: 60,
      tradeFeeRate: 2500,
      protocolFeeRate: 12000,
    },
  ],
  "create-pool": [
    {
      ammConfig: "3DxyRBpXLAkCkPewZjDNb7ysb6h9GyXw83fcNGmmUQgP",
      tokenMint0: "So11111111111111111111111111111111111111112",
      tokenMint1: "6pc7UqAjU4guU8pmimv1iCf5oDphQB5dANFmD7UvwcYk",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      priceLower: new Decimal("30"),
      priceUpper: new Decimal("40"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      priceLower: new Decimal("50"),
      priceUpper: new Decimal("60"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "EMeXUwceGHbyLRZ8SpPXYhkoWq6cZw5QyfMFhpZiqM8k",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "HKK3PkqjXMm7acNw6L6UacG5pA2ArPHsuSa3CnE4tT5u",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "4m8gBWCMP88rTbWDcDJjkpWMSD6J7SHohmwyzYW4zZhZ",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "EMeXUwceGHbyLRZ8SpPXYhkoWq6cZw5QyfMFhpZiqM8k",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "HKK3PkqjXMm7acNw6L6UacG5pA2ArPHsuSa3CnE4tT5u",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      positionId: "4m8gBWCMP88rTbWDcDJjkpWMSD6J7SHohmwyzYW4zZhZ",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "swap-base-in": [
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      inputTokenMint: "6pc7UqAjU4guU8pmimv1iCf5oDphQB5dANFmD7UvwcYk",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
  ],
  "swap-base-out": [
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
    {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      outputTokenMint: "6pc7UqAjU4guU8pmimv1iCf5oDphQB5dANFmD7UvwcYk",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "8PHbPyeLZeXAMgvtWxMZt9gePvzvXP9RbfN4aTEGSfHG",
      inputTokenMint: "6pc7UqAjU4guU8pmimv1iCf5oDphQB5dANFmD7UvwcYk",
    },
    remainRouterPoolIds: ["HdT56w2iJqob9eVZDWhjZ6cyYsbuWLhLe7a8zyuhy6q7"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
};
