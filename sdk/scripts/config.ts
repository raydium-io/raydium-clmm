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
      tokenMint1: "9mxCo5ysB576YJppPhLh8WQYLMDNziUihWmVoKFhVVWz",
      initialPrice: new Decimal("44"),
    },
    {
      ammConfig: "3DxyRBpXLAkCkPewZjDNb7ysb6h9GyXw83fcNGmmUQgP",
      tokenMint0: "GwNdKAvxBD4jg9qNgHZ2QAMwrytASNV6KezE4na7PrZf",
      tokenMint1: "9mxCo5ysB576YJppPhLh8WQYLMDNziUihWmVoKFhVVWz",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      priceLower: new Decimal("30"),
      priceUpper: new Decimal("40"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      priceLower: new Decimal("50"),
      priceUpper: new Decimal("60"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8GNRqLUZdL4TTmjH6Mk4QpGoHWn7RAq33G7CJzE76fip",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "4ifU51AnLj5YzkVnxKmSmdMWncFw3zaYPXhfD8c83tXU",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "4YN1Uvcf4GKvHzESdYwpfCw3CuGLXABrx8g1yWzKGRjQ",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "CWnWKtn4DLPj6nD6YSUNUToDk1FNQkbrrwLBed3GkQR9",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "4ifU51AnLj5YzkVnxKmSmdMWncFw3zaYPXhfD8c83tXU",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "4YN1Uvcf4GKvHzESdYwpfCw3CuGLXABrx8g1yWzKGRjQ",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      positionId: "CWnWKtn4DLPj6nD6YSUNUToDk1FNQkbrrwLBed3GkQR9",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "swap-base-in": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      inputTokenMint: "9mxCo5ysB576YJppPhLh8WQYLMDNziUihWmVoKFhVVWz",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
  ],
  "swap-base-out": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      outputTokenMint: "9mxCo5ysB576YJppPhLh8WQYLMDNziUihWmVoKFhVVWz",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      inputTokenMint: "So11111111111111111111111111111111111111112",
    },
    remainRouterPoolIds: ["8GNRqLUZdL4TTmjH6Mk4QpGoHWn7RAq33G7CJzE76fip"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
  "initialize-reward": [
    {
      poolId: "8Mk4D4CMeauA3yDDYCW2rgGMBgz3WpLqG6J8WkgEhn7w",
      rewardTokenMint: "447jZ4hRB9ZmziuRBZkMFnMSiw3rPKcPrYfio2uBeD4c",
      rewardIndex: 0,
      openTime: new BN(1662438641),
      endTime: new BN(1663302641),
      emissionsPerSecond: 0.1,
    }
  ],
};
