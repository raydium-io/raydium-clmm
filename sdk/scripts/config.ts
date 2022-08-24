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
      tokenMint0: "So11111111111111111111111111111111111111112",
      tokenMint1: "BLYTbdZGESHS7ZJRiKqt6nVcLnyo3LNCvw721GDSqK2p",
      initialPrice: new Decimal("44"),
    },
  ],
  "open-position": [
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      priceLower: new Decimal("11"),
      priceUpper: new Decimal("88"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      priceLower: new Decimal("30"),
      priceUpper: new Decimal("40"),
      liquidity: new BN("100000000"),
      amountSlippage: 0.005,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      priceLower: new Decimal("50"),
      priceUpper: new Decimal("60"),
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "increase-liquidity": [
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "6vySZXs5PpS2PberzDEK1LN5UMugMGUuiXQUVVQk8EZs",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "GUtFrZ3M3X16ej1oasrxRuVDcopTSsPjY8jo5uJNYXEs",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "E5GstZnExh4LTd3RKTT3dnsHwbCHs3CbPn9WjnbcpRDB",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "decrease-liquidity": [
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "6vySZXs5PpS2PberzDEK1LN5UMugMGUuiXQUVVQk8EZs",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "GUtFrZ3M3X16ej1oasrxRuVDcopTSsPjY8jo5uJNYXEs",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      positionId: "E5GstZnExh4LTd3RKTT3dnsHwbCHs3CbPn9WjnbcpRDB",
      liquidity: new BN("100000000"),
      amountSlippage: 0,
    },
  ],
  "swap-base-in": [
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      inputTokenMint: "So11111111111111111111111111111111111111112",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      inputTokenMint: "BLYTbdZGESHS7ZJRiKqt6nVcLnyo3LNCvw721GDSqK2p",
      amountIn: new BN("1000000"),
      priceLimit: new Decimal(0),
      amountOutSlippage: 0,
    },
  ],
  "swap-base-out": [
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      outputTokenMint: "So11111111111111111111111111111111111111112",
      amountOut: new BN("100000"),
      priceLimit: new Decimal(0),
      amountInSlippage: 0,
    },
    {
      poolId: "Av8WbGwUGSfRHPUoaPAKocGAvvb9sZTuQEwKVBKwUQXL",
      outputTokenMint: "BLYTbdZGESHS7ZJRiKqt6nVcLnyo3LNCvw721GDSqK2p",
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
