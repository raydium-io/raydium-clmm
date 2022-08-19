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
      tokenMint0: "5rRx7xYrGGGwC82nBULvanz78ykypdXXWCnQ86ctJKup",
      tokenMint1: "AfomcgrX8p8idhMndNi7eBBgweLVDtNZJFzSmUtFUbGA",
      initialPrice: new Decimal("1"),
    },
    {
      ammConfig: "47QDZdQvRQtAutMRWCLe7FRNGegjEmcqhG874aj3k9HT",
      tokenMint0: "So11111111111111111111111111111111111111112",
      tokenMint1: "AfomcgrX8p8idhMndNi7eBBgweLVDtNZJFzSmUtFUbGA",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "4gzWbm3BamDNMtksKAA4ek9sA4mp8tNHjrWtCW2NdbWA",
      priceLower: new Decimal("0.5"),
      priceUpper: new Decimal("1.5"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "4gzWbm3BamDNMtksKAA4ek9sA4mp8tNHjrWtCW2NdbWA",
      positionId: "9c8GcbHxHTpDLveS7yabTi6H2ckqequkCChDP34ujtt8",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      positionId: "4F4ppcX7Bu8EbiPMnMB9mG2pNYUTtKhA63ncBNnqNrDs",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "4gzWbm3BamDNMtksKAA4ek9sA4mp8tNHjrWtCW2NdbWA",
      positionId: "9c8GcbHxHTpDLveS7yabTi6H2ckqequkCChDP34ujtt8",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      positionId: "4F4ppcX7Bu8EbiPMnMB9mG2pNYUTtKhA63ncBNnqNrDs",
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
  ],
  "swap-base-in": [
    {
      poolId: "4gzWbm3BamDNMtksKAA4ek9sA4mp8tNHjrWtCW2NdbWA",
      inputTokenMint: "5rRx7xYrGGGwC82nBULvanz78ykypdXXWCnQ86ctJKup",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      inputTokenMint: "AfomcgrX8p8idhMndNi7eBBgweLVDtNZJFzSmUtFUbGA",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0.005,
    },
  ],
  "swap-base-out": [
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
    {
      poolId: "8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd",
      outputTokenMint: "AfomcgrX8p8idhMndNi7eBBgweLVDtNZJFzSmUtFUbGA",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0.005,
    },
  ],
  "swap-router-base-in": {
    startPool: {
      poolId: "4gzWbm3BamDNMtksKAA4ek9sA4mp8tNHjrWtCW2NdbWA",
      inputTokenMint: "5rRx7xYrGGGwC82nBULvanz78ykypdXXWCnQ86ctJKup",
    },
    remainRouterPoolIds: ["8MK4MG3CC2yybwFUjzsUzSJztPMBDKLPXvKdERki1rtd"],
    amountIn: new BN("100000"),
    amountOutSlippage: 0.005,
  },
};
