Raydium-Amm-v3 is an open-sourced concentrated liquidity market maker (CLMM) program built for the Solana ecosystem.

**Concentrated Liquidity Market Maker (CLMM)** pools allow liquidity providers to select a specific price range at which liquidity is active for trades within a pool. This is in contrast to constant product Automated Market Maker (AMM) pools, where all liquidity is spread out on a price curve from 0 to ∞. For LPs, CLMM design enables capital to be deployed with higher efficiency and earn increased yield from trading fees. For traders, CLMMs improve liquidity depth around the current price which translates to better prices and lower price impact on swaps. CLMM pools can be configured for pairs with different volatility.

## Environment Setup
1. Install [Rust](https://www.rust-lang.org/tools/install).
2. Install [Solana](https://docs.solana.com/cli/install-solana-cli-tools) and then run `solana-keygen new` to create a keypair at the default location.
3. install [Anchor](https://book.anchor-lang.com/getting_started/installation.html).

## Quickstart

Clone the repository and enter the source code directory.
```
git clone https://github.com/raydium-io/raydium-amm-v3
cd raydium-amm-v3
```

Build
```
anchor build
```
After building, the smart contract files are all located in the target directory.

Deploy
```
anchor deploy
```
Attention, check your configuration and confirm the environment you want to deploy.

# License
The source code is licensed under Apache 2.0.