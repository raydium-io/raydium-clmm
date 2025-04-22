# Raydium AMM V3: Concentrated Liquidity Market Maker

## Project Overview

Raydium-Amm-v3 is an open-sourced Concentrated Liquidity Market Maker (CLMM) program specifically designed for the Solana blockchain ecosystem. This innovative decentralized exchange (DEX) protocol introduces advanced liquidity provisioning mechanisms that significantly improve capital efficiency and trading performance.

### Key Innovations

- **Concentrated Liquidity**: Unlike traditional AMMs where liquidity is spread across an infinite price range, Raydium AMM V3 allows liquidity providers to:
  - Select specific price ranges for active liquidity
  - Deploy capital more efficiently
  - Earn increased yields from trading fees

- **Benefits for Traders and Liquidity Providers**:
  - Improved liquidity depth around current market prices
  - Lower price impact on swaps
  - Flexible pool configurations for different asset volatilities

## Getting Started

### Prerequisites

1. **Rust Toolchain**
   ```shell
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup default 1.79.0
   ```

2. **Solana CLI**
   ```shell
   sh -c "$(curl -sSfL https://release.solana.com/v1.17.0/install)"
   solana-keygen new  # Create keypair
   ```

3. **Anchor Framework**
   ```shell
   cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
   avm install 0.29.0
   ```

### Installation

1. Clone the repository
   ```shell
   git clone https://github.com/raydium-io/raydium-amm-v3
   cd raydium-amm-v3
   ```

2. Build the project
   ```shell
   anchor build
   ```

3. Deploy the smart contract
   ```shell
   anchor deploy
   ```
   > **Note**: Carefully verify your deployment configuration

## Project Structure

```
raydium-amm-v3/
│
├── client/                   # Client-side implementation
│   └── src/
│       ├── instructions/     # RPC and instruction handlers
│       └── main.rs
│
├── programs/amm/             # Core AMM program
│   ├── src/
│   │   ├── instructions/     # Program instructions (swap, liquidity, etc.)
│   │   ├── libraries/        # Mathematical and utility libraries
│   │   ├── states/           # Program state definitions
│   │   └── lib.rs
│
├── Anchor.toml               # Anchor configuration
└── Cargo.toml                # Rust project dependencies
```

## Technologies Used

- **Programming Languages**: 
  - Rust
  - Solidity (for smart contract development)

- **Blockchain Platform**: 
  - Solana

- **Frameworks & Tools**:
  - Anchor
  - Solana CLI
  - Cargo (Rust package manager)

## Cross-Program Invocation (CPI)

For an example of calling CLMM programs, refer to the [Raydium CPI Example](https://github.com/raydium-io/raydium-cpi-example/tree/master/clmm-cpi)

## Usage Examples

Detailed usage examples can be found in the project's documentation and the CPI example repository.

## Contributing

Contributions are welcome! Please read the contributing guidelines and code of conduct before submitting pull requests.

## License

This project is licensed under the Apache License 2.0. See the [LICENSE](LICENSE) file for complete details.

## Disclaimer

This software is provided "as is" without warranty. Users should conduct their own due diligence and understand the risks associated with decentralized finance (DeFi) protocols.