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
  // url: "https://api.mainnet-beta.solana.com",
  url: "https://raydium-cranking.rpcpool.com/13bb6d7c668753052cdcc23aaaf6",
  programId: new PublicKey("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK"),
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
      tokenMint0: "FQKdmijah3S8j8RzBxUjGcBQ5DXQJMfHCD2FqGyh7ioC",
      tokenMint1: "FfySf3EP8jRXNrUmoKyb1uej89ZJufLTQLEoK2kreYFQ",
      initialPrice: new Decimal("1"),
    },
  ],
  "open-position": [
    {
      poolId: "61R1ndXxvsWXXkWSyNkCxnzwd3zUNB8Q2ibmkiLPC8ht",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      positionId: "4aG7pFXYBRNLghPQykKphkgzVsQAhUkoTB9xQ8XX21qZ",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "CrhoHr8h7553wzQMWzFu2KFS9cpbgPJhKTH86JD4gTAX",
      positionId: "4aG7pFXYBRNLghPQykKphkgzVsQAhUkoTB9xQ8XX21qZ",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "swap-base-in": [
    {
      poolId: "61R1ndXxvsWXXkWSyNkCxnzwd3zUNB8Q2ibmkiLPC8ht",
      inputTokenMint: "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",
      amountIn: new BN("1000000"),
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
