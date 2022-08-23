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
  programId: new PublicKey("devKfPVu9CaDvG47KG7bDKexFvAY37Tgp6rPHTruuqU"),
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
      ammConfig: "47QDZdQvRQtAutMRWCLe7FRNGegjEmcqhG874aj3k9HT",
      tokenMint0: "2psaEzJ4rWXf9Ywockmb8Q8R6xx76Nwmz4uLjm7HsKaa",
      tokenMint1: "DunbyXRAAo8amM2aYV2bewuEzGZ2XSpwsiA52B1cwivz",
      initialPrice: new Decimal("1"),
    },
    {
      ammConfig: "47QDZdQvRQtAutMRWCLe7FRNGegjEmcqhG874aj3k9HT",
      tokenMint0: "So11111111111111111111111111111111111111112",
      tokenMint1: "DunbyXRAAo8amM2aYV2bewuEzGZ2XSpwsiA52B1cwivz",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "6Pno6JVhzfjYC53GYGjcNk6a7SKFGnu1SXsgrFSUVzh4",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "6Pno6JVhzfjYC53GYGjcNk6a7SKFGnu1SXsgrFSUVzh4",
      positionId: "DaJ9Ma9BbHemQwhCxqzm7pLESivQVaq8JHu7SQKwazC2",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      positionId: "HAKzBTYHSEdKTs16mDtCtYRvRoGuNChjcdcvJryWoW4s",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "6Pno6JVhzfjYC53GYGjcNk6a7SKFGnu1SXsgrFSUVzh4",
      positionId: "DaJ9Ma9BbHemQwhCxqzm7pLESivQVaq8JHu7SQKwazC2",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      positionId: "HAKzBTYHSEdKTs16mDtCtYRvRoGuNChjcdcvJryWoW4s",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "swap-base-in": [
    {
      poolId: "6Pno6JVhzfjYC53GYGjcNk6a7SKFGnu1SXsgrFSUVzh4",
      inputTokenMint: "2psaEzJ4rWXf9Ywockmb8Q8R6xx76Nwmz4uLjm7HsKaa",
      amountIn: new BN("10000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      inputTokenMint: "DunbyXRAAo8amM2aYV2bewuEzGZ2XSpwsiA52B1cwivz",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
  ],
  "swap-base-out": [
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
    {
      poolId: "BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB",
      outputTokenMint: "DunbyXRAAo8amM2aYV2bewuEzGZ2XSpwsiA52B1cwivz",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "6Pno6JVhzfjYC53GYGjcNk6a7SKFGnu1SXsgrFSUVzh4",
      inputTokenMint: "2psaEzJ4rWXf9Ywockmb8Q8R6xx76Nwmz4uLjm7HsKaa",
    },
    remainRouterPoolIds: ["BvT4mQbsaLmuy7cn5JjBv3TSwpwCbGVwF76DyY7aPBoB"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
};
