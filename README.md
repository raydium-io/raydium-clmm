# Raydium AMM Protocol v3

Concentrated liquidity on Solana

## High level design

The main challenge was to adapt Uniswap's architecture to sealevel, and EVM's 256 bit numbers to the 64 bit rust runtime.

### 1. Architecture

- Smart contracts of importance on Uniswap are

    1. **Factory**: Creates pools for a given pair of tokens and fee tier. Also allows the creation of new fee tiers and changing the protocol owner.
    2. **Pool**: Lower level API to create positions linked with public keys, and perform swaps. Swaps can only be performed by another smart contract implementing the swap callback API.
    3. **Non fungible position manager**: Interacts with the core pool, creating positions for the user tied to non-fungible tokens (NFTs).
    4. **Swap router**: Supports advanced swap features like deadlines, slippage checks and exact input / exact output swaps. It implements the swap callback API as required by the core.

- Raydium had to adapt Uniswap's architecture to meet Sealevel's pecularieties like:
    1. Smart contracts cannot deploy other smart contracts
    2. Separation of data and business logic: Solana programs are stateless. Accounts are used to persist data
    3. Re-entrancy is limited to self-recursion, i.e. we cannot not replicate the swap callback pattern if two separate smart contracts are used
    4. High compute budget cost of cross program invocations and the 200k compute unit limitation
    5. Account lookups must be performed client side. For example we must derive the correct observation account on client side and pass it via context, instead of the smart contract deriving it at runtime.

- The following design decisions were made:
    1. A monolithic program, instead of multiple smart contracts
    2. An internal function call was used instead of self-recursion for the swap callback to save compute units
    3. Program derived addresses replace maps and arrays. Instead of pool addresses being tracked by a (token0, token1, fee) map we use PDAs with a similar address derivation scheme. The oracle observation array is similarly emulated using array index as a seed.


### 2. Mathematics

1. Logarithm and power functions have been adapted to 64 bit by scaling down magic numbers.
2. The variable scaling factor is 1/4 with some exceptions:
    - Token amounts: 64 bit (dictated by SPL token program) in place of 256 bit
    - Square root price: `sqrt_price_x32: u64` in place of `uint160 sqrtPriceX96`. Effectively this is 32 bit `√P` with 32 bits for decimal places (U32.32 format).
    - Liquidity: `u64` in place of `u128`. Given `x = L/√P`, `y = L√P` with x,y (amounts) being 64 bit, the product or division of `L` and `√P` can never exceed 64 bits. 64 and not 32 bits were used for liquidity so that max liquidity per tick could be respectable.
3. There's no mulmod opcode equivalent in Rust, so we implement phantom overflow resistant mul-div using large numbers(U128 for u64, U256 for U128) in the full_math library.
4. `U128` used in place of the native `u128` for [compute unit efficiency](https://github.com/solana-labs/solana/issues/19549).

Note that the math libraries in /libraries have 100% test coverage.

### Directory structure

- [lib.rs](./programs/core/src/lib.rs): Smart contract instructions
- [context.rs](./programs/core/src/context.rs): Accounts required for each instruction
- [error.rs](./programs/core/src/error.rs): Error codes. Cyclos tries to preserve Uniswap's convention on error messages.
- [access_control.rs](./programs/core/src/access_control.rs): Deadline and authorization checks
- [/libraries](./programs/core/src/libraries): Stateless math libraries
- [/states](./programs/core/src/states): Various accounts (factory, pool, position etc) and their associated functions

## Test coverage

- We tried to port all of Uniswap's tests to inherit its security.
    - [/libraries](./programs/core/src/libraries) has 100% coverage
    - [/states](./programs/core/src/states): oracle, pool, position_manager and swap_router are left out unfortunately

- Difficulty arose in testing due to Solana's account model. Rust based unit tests could not be written for parts like the oracle, due to client side lookup. These will be re-written as JS based unit tests.

- Some issues were discovered during client side development and hot-patched.
    - https://github.com/cyclos-io/cyclos-protocol-v2/commit/2e21a3a2c3100ba73860e4ae8b2481dfd0c15a7c
    - https://github.com/cyclos-io/cyclos-protocol-v2/commit/df33e0cffac2d085bdb85f8d33500ba12131a499

## Resources

- Account diagram and library tree: https://drive.google.com/file/d/1S8LMa22uxBh7XGNMUzp-DDhVhE-G9S2s/view?usp=sharing

- Task tracker: https://github.com/orgs/cyclos-io/projects/1

## Oracle design

- Cyclos adapts Uniswap's circular observation array to Sealevel using program derived accounts (PDA). Each PDA is seeded with an **index** representing its array position in the given way-

```
[OBSERVATION_SEED, token_0, token_1, fee, index]
```

- Index is incremented for every successive observation and wrapped around at the end element. For a cardinality 3, the indexes will be `0, 1, 2, 0, 1` and so on. The index for the latest position is found as `pool.observation_index % cardinality`.

- Cardinality can be grown to store more observations.
    1. Created slots are [marked as uninitialized](https://github.com/Uniswap/v3-core/blob/ed88be38ab2032d82bf10ac6f8d03aa631889d48/contracts/libraries/Oracle.sol#L117). A placeholder timestamp value of 1 is stored to perform an SSTORE and pay for slot creation. Cyclos analogously creates a program account.
    2. The pool variable `observationCardinality` stores the number of **initialized slots**, and `observationCardinalityNext` stores the count of **created slots**. `observationCardinalityNext` is incremented on slot creation, but not `observationCardinality`.
    3. When we reach the end element allowed by `observationCardinality`, the value of this variable is incremented so that the next uninitialized slot can be reached for writing the next observation. This repeats until every uninitialized slot is filled.

- Obervations are updated on
    1. [Swaps](./programs/core/src/lib.rs#L1483)
    2. [Position modifications, i.e. creating, removing and modifying positions](./programs/core/src/lib.rs#L2387)

- Uniswap checkpoints data whenever a pool is touched **for the first time in a block**. Other interactions within the block are not recorded.

- Uniswap's observation array can store 65k slots, equivalent to 9 days of recordings given Ethereum's 14 second block time. 65k slots would result in just a day's worth of readings on Solana given its 0.5 second block time. We introduce a time partitioning mechanism to overcome this limitation

    1. Block time is partitioned in 14 second intervals starting with epoch time 0.
    ```
    |----partition_0 [0, 14)----|----partition_1 [14, 28)----|----partition_0 [28, 42)----|
    ```

    2. To know the partition for a timestamp, perform floor division by 14.

    ```
    partition = floor(timestamp / 14)
    ```

    3. Find the partitions for the current block time (partition_current) and for the last saved observation (partition_last).

    4. If `partition_current > partition_last` checkpoint in the next slot. Else overwrite the current slot with new data.

- Unlike EVM, the last and next observation accounts must be found on client side in Sealevel.
    1. Last observation state: Acccount storing the last checkpoint.
    2. Next observation state: The account which follows the last observation, given by formula `(index_last + 1) % cardinality_next`. This account is read/modfified only if the next and last checkpoint fall in different partitions. This field can be made optional in future by using remaining accounts.

#### Optimize build

1. Repo wide Cargo.toml

```toml
[profile.release]
lto = "fat"
codegen-units = 1

[profile.release.build-override]
opt-level = 3
incremental = false
codegen-units = 1
```

2. Build with `anchor test -- --features no-log-ix-name` to disable function name logging
