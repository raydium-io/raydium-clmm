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
      tokenMint0: "5x7dAB4HfSn4FrFRPeNJF4bpvfTzaU2gEg2xRbUhqaYF",
      tokenMint1: "82xhaNPDQt7WAwyH2ZHXeSaXtb9LhzEKPnad1tWDiVDX",
      initialPrice: new Decimal("1"),
    },
    {
      ammConfig: "47QDZdQvRQtAutMRWCLe7FRNGegjEmcqhG874aj3k9HT",
      tokenMint0: "82xhaNPDQt7WAwyH2ZHXeSaXtb9LhzEKPnad1tWDiVDX",
      tokenMint1: "Aa2dSbfjTb45LxuTN8c8TvJuVbas8zL4VyXYRgKFocVG",
      initialPrice: new Decimal("1"),
    },
  ],
  "open-position": [
    {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "CVr8pKUM893U7vAirhYiFm4MTp2vdAg85q5x7LTYuWkK",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      positionId: "48jdUMHRCHhiHDWVNo4snrYndZh7gwXgGhyK5bmsYw1W",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      positionId: "48jdUMHRCHhiHDWVNo4snrYndZh7gwXgGhyK5bmsYw1W",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "swap-base-in": [
    {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      inputTokenMint: "5x7dAB4HfSn4FrFRPeNJF4bpvfTzaU2gEg2xRbUhqaYF",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
  ],
  "swap-base-out": [
    {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      outputTokenMint: "82xhaNPDQt7WAwyH2ZHXeSaXtb9LhzEKPnad1tWDiVDX",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "5a8xUFnpCHP9kz2ZaA1pjcS31HEyGVzaygyv6FiVeUZC",
      inputTokenMint: "5x7dAB4HfSn4FrFRPeNJF4bpvfTzaU2gEg2xRbUhqaYF",
    },
    remainRouterPoolIds: ["CVr8pKUM893U7vAirhYiFm4MTp2vdAg85q5x7LTYuWkK"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
};
