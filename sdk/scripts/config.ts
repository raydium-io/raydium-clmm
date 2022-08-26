import { BN } from "@project-serum/anchor";
import { ConfirmOptions, PublicKey } from "@solana/web3.js";
import Decimal from "decimal.js";

export const defaultConfirmOptions: ConfirmOptions = {
  preflightCommitment: "processed",
  commitment: "processed",
  skipPreflight: true,
};

export const Config = {
  // url: "https://api.devnet.solana.com",
  url: "http://127.0.0.1:8899",
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
      tokenMint1: "6428HwUQB1qiAFkjCMiNXbCtUuZp7DARqJJrbJZrpusQ",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      priceLower: new Decimal("30"),
      priceUpper: new Decimal("40"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      priceLower: new Decimal("50"),
      priceUpper: new Decimal("60"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "4kMGCBPE2zzpqoWuqMCdVRg6YUSVEsiZDuFenVmzb8zk",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "ErAeEQVRorAUq8HmfA8R4CGrhwas9WCkHW8xLHibLgVY",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "2z9eRcNExgYyx7yoWFxUey3s7VVLGuft9fk4xaGUESRn",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "4kMGCBPE2zzpqoWuqMCdVRg6YUSVEsiZDuFenVmzb8zk",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "ErAeEQVRorAUq8HmfA8R4CGrhwas9WCkHW8xLHibLgVY",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      positionId: "2z9eRcNExgYyx7yoWFxUey3s7VVLGuft9fk4xaGUESRn",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "swap-base-in": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      inputTokenMint: "J7MaBm5n5mhZKySz39towHZLeNgWGFEZMsvPG4PRBrY2",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
  ],
  "swap-base-out": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      outputTokenMint: "J7MaBm5n5mhZKySz39towHZLeNgWGFEZMsvPG4PRBrY2",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      inputTokenMint: "J7MaBm5n5mhZKySz39towHZLeNgWGFEZMsvPG4PRBrY2",
    },
    remainRouterPoolIds: ["HdT56w2iJqob9eVZDWhjZ6cyYsbuWLhLe7a8zyuhy6q7"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
  "initialize-reward": [
    // {
    //   poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
    //   rewardTokenMint: "J7MaBm5n5mhZKySz39towHZLeNgWGFEZMsvPG4PRBrY2",
    //   rewardIndex: 0,
    //   openTime: new BN(1661493622),
    //   endTime:  new BN(1661493625),
    //   emissionsPerSecond: 1, 
    // },
    // {
    //   poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
    //   rewardTokenMint: "5mY1xvEXEGgvqjN1GGRsLtSKGHcPJ6fKEkDnFigM2oBW",
    //   rewardIndex: 1,
    //   openTime: new BN(1661503428),
    //   endTime:  new BN(1661503488),
    //   emissionsPerSecond: 1,
    // },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      rewardTokenMint: "GoCGYHggzc8Bskc2YGRvrwobcmhX5CMroWDpvADUqM6U",
      rewardIndex: 2,
      openTime: new BN(1661495392),
      endTime:  new BN(1661495395),
      emissionsPerSecond: 1,
    },
  ],
  "set-reward-emissions": [
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      rewardIndex: 0,
      emissionsPerSecond: 0.2, 
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      rewardIndex: 1,
      emissionsPerSecond: 0.2,
    },
    {
      poolId: "HnXA7A3U7yTyKvSrRHhrZyZmUx9hmUQGaZUEMZ9eFWgs",
      rewardIndex: 2,
      emissionsPerSecond: 0.2,
    },
  ],
};
