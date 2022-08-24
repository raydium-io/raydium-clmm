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
      ammConfig: "CP7fpJt1y43ExsUUp269wdQH6tfLHPupaDToeVJgbgjh",
      tokenMint0: "5iKLWCDrscNcpH4QppYUXCi8aju25MRFmQKGJzDx1hkE",
      tokenMint1: "HiFJGEg878HXXBKPVcjmwwwxfVrPwgBLMgyPDgYs9Pq3",
      initialPrice: new Decimal("140"),
    },
    // {
    //   ammConfig: "47QDZdQvRQtAutMRWCLe7FRNGegjEmcqhG874aj3k9HT",
    //   tokenMint0: "So11111111111111111111111111111111111111112",
    //   tokenMint1: "DunbyXRAAo8amM2aYV2bewuEzGZ2XSpwsiA52B1cwivz",
    //   initialPrice: new Decimal("44"),
    // },
  ],
  "open-position": [
    {
      poolId: "FcVZ6kzdHHuTeK3oRiSZiefNEaeayJxoqishm4vKZE6R",
      priceLower: new Decimal("110"),
      priceUpper: new Decimal("220"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "FcVZ6kzdHHuTeK3oRiSZiefNEaeayJxoqishm4vKZE6R",
      positionId: "FzKNkZ1FJQXKAKUKzHBckEhZFRwUkGQ36wHF2rahahsS",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "FcVZ6kzdHHuTeK3oRiSZiefNEaeayJxoqishm4vKZE6R",
      positionId: "FzKNkZ1FJQXKAKUKzHBckEhZFRwUkGQ36wHF2rahahsS",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "swap-base-in": [
    {
      poolId: "FcVZ6kzdHHuTeK3oRiSZiefNEaeayJxoqishm4vKZE6R",
      inputTokenMint: "5iKLWCDrscNcpH4QppYUXCi8aju25MRFmQKGJzDx1hkE",
      amountIn: new BN("100000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0
    },
  ],
  "swap-base-out": [
    {
      poolId: "FcVZ6kzdHHuTeK3oRiSZiefNEaeayJxoqishm4vKZE6R",
      outputTokenMint: "5iKLWCDrscNcpH4QppYUXCi8aju25MRFmQKGJzDx1hkE",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      inputTokenMint: "BLYTbdZGESHS7ZJRiKqt6nVcLnyo3LNCvw721GDSqK2p",
    },
    remainRouterPoolIds: ["HdT56w2iJqob9eVZDWhjZ6cyYsbuWLhLe7a8zyuhy6q7"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
};
