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
      tokenMint0: "6BsnRfuAfhxPY91wF7Q14iy295Ga5V3mVzhdnBY9KfnS",
      tokenMint1: "6ajT55d5NXQKqTjmcumTaa1AiF2t3y2XYSbjCoq66zU2",
      initialPrice: new Decimal("1"),
    },
  ],
  "open-position": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      token0Amount: new BN("1000000"),
      token1Amount: new BN("1000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      positionId: "4aG7pFXYBRNLghPQykKphkgzVsQAhUkoTB9xQ8XX21qZ",
      token0Amount: new BN("1000000"),
      token1Amount: new BN("1000000"),
      amountSlippage: 0.005,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      positionId: "4aG7pFXYBRNLghPQykKphkgzVsQAhUkoTB9xQ8XX21qZ",
      token0Amount: new BN("1000000"),
      token1Amount: new BN("1000000"),
      amountSlippage: 0.005,
    },
  ],
  "swap-base-in": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      inputTokenMint: "6BsnRfuAfhxPY91wF7Q14iy295Ga5V3mVzhdnBY9KfnS",
      amountIn: new BN("100000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
  ],
  "swap-base-out": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      outputTokenMint: "6ajT55d5NXQKqTjmcumTaa1AiF2t3y2XYSbjCoq66zU2",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
  ],
};
