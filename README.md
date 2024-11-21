Raydium-Amm-v3 is an open-sourced concentrated liquidity market maker (CLMM) program built for the Solana ecosystem.

**Concentrated Liquidity Market Maker (CLMM)** pools allow liquidity providers to select a specific price range at which liquidity is active for trades within a pool. This is in contrast to constant product Automated Market Maker (AMM) pools, where all liquidity is spread out on a price curve from 0 to ∞. For LPs, CLMM design enables capital to be deployed with higher efficiency and earn increased yield from trading fees. For traders, CLMMs improve liquidity depth around the current price which translates to better prices and lower price impact on swaps. CLMM pools can be configured for pairs with different volatility.

## Environment Setup
1. Install `Rust`

   ```shell
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default 1.79.0
   ```

2. Install `Solana `

   ```shell
   sh -c "$(curl -sSfL https://release.solana.com/v1.17.0/install)"
   ```

   then run `solana-keygen new` to create a keypair at the default location.

3. install `Anchor`

   ```shell
   # Installing using Anchor version manager (avm) 
   cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
   # Install anchor
   avm install 0.29.0
   ```

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

# CPI

An example of calling clmm can be found [here](https://github.com/raydium-io/raydium-cpi-example/tree/master/clmm-cpi)

# License
The source code is [licensed](https://github.com/raydium-io/raydium-clmm/blob/master/LICENSE) under Apache 2.0.