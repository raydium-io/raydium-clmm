# Raydium CLMM Bug Bounty Program

Raydium's full bug bounty program with ImmuneFi can be found at: https://immunefi.com/bounty/raydium/

## Rewards by Threat Level

Rewards are distributed according to the impact of the vulnerability based on the Immunefi Vulnerability Severity Classification System V2.3. This is a simplified 5-level scale, focusing on the impact of the vulnerability reported.

### Smart Contracts

| Severity | Bounty                    |
| -------- | ------------------------- |
| Critical | USD 50,000 to USD 505,000 |
| High     | USD 40,000                |
| Medium   | USD 5,000                 |

All bug reports must include a Proof of Concept (PoC) demonstrating how the vulnerability can be exploited to impact an asset-in-scope to be eligible for a reward. Critical and High severity bug reports should also include a suggestion for a fix. Explanations and statements are not accepted as PoC and code is required.

Rewards for critical smart contract bug reports will be further capped at 10% of direct funds at risk if the bug discovered is exploited. However, there is a minimum reward of USD 50,000.

Bugs in `raydium-sdk` and other code outside of the smart contract will be assessed on a case-by-case basis.

## Report Submission

Please email security@reactorlabs.io with a detailed description of the attack vector. For high- and critical-severity reports, please include a proof of concept. We will reach back out within 24 hours with additional questions or next steps on the bug bounty.

## Payout Information

Payouts are handled by the Raydium team directly and are denominated in USD. Payouts can be done in RAY, SOL, or USDC.

## Out of Scope & Rules

The following vulnerabilities are excluded from the rewards for this bug bounty program:

- Attacks that the reporter has already exploited themselves, leading to damage
- Attacks requiring access to leaked keys/credentials
- Attacks requiring access to privileged addresses (governance, strategist)
- Incorrect data supplied by third party oracles (not excluding oracle manipulation/flash loan attacks)
- Basic economic governance attacks (e.g. 51% attack)
- Lack of liquidity
- Best practice critiques
- Sybil attacks
- Centralization risks
- Any UI bugs
- Bugs in the core Solana runtime (please submit these to [Solana's bug bounty program](https://github.com/solana-labs/solana/security/policy))
- Vulnerabilities that require a validator to execute them
- Vulnerabilities requiring access to privileged keys/credentials
- MEV vectors the team is already aware of
- The CLMM contract emits trading fee and farming yield tokens to LPs. If tokens from the vault or fees were drained by an attacker however, users would not be able to claim yield and transactions would fail. This is by design and not a vulnerability.

## Concentrated Liquidity Assets in Scope

| Target                                                                                                                   | Type                                       |
| ------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/collect_fund_fee.rs         | Smart Contract - collect_fund_fee          |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/collect_protocol_fee.rs     | Smart Contract - collect_protocol_fee      |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/create_operation_account.rs | Smart Contract - create_operation_account  |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/mod.rs                      | Smart Contract - admin/mod                 |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/transfer_reward_owner.rs    | Smart Contract - transfer_reward_owner     |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/update_amm_config.rs        | Smart Contract - update_amm_config         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/update_operation_account.rs | Smart Contract - update_operation_account  |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/admin/update_pool_status.rs       | Smart Contract - update_pool_status        |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/close_position.rs                 | Smart Contract - close_position            |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/collect_remaining_rewards.rs      | Smart Contract - collect_remaining_rewards |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/create_pool.rs                    | Smart Contract - create_pool               |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/decrease_liquidity.rs             | Smart Contract - decrease_liquidity        |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/increase_liquidity.rs             | Smart Contract - increase_liquidity        |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/initialize_reward.rs              | Smart Contract - initialize_reward         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/mod.rs                            | Smart Contract - instructions/mod          |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/open_position.rs                  | Smart Contract - open_position             |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/set_reward_params.rs              | Smart Contract - set_reward_params         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/swap.rs                           | Smart Contract - swap                      |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/swap_router_base_in.rs            | Smart Contract - swap_router_base_in       |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/update_reward_info.rs             | Smart Contract - update_reward_info        |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/big_num.rs                           | Smart Contract - big_num                   |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/fixed_point_64.rs                    | Smart Contract - fixed_point               |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/full_math.rs                         | Smart Contract - full_math                 |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/liquidity_math.rs                    | Smart Contract - liquidity_math            |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/mod.rs                               | Smart Contract - libraries/mod             |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/sqrt_price_math.rs                   | Smart Contract - sqrt_price_math           |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/swap_math.rs                         | Smart Contract - swap_math                 |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/tick_array_bit_map.rs                | Smart Contract - tick_array_bit_map        |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/tick_math.rs                         | Smart Contract - tick_math                 |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/libraries/unsafe_math.rs                       | Smart Contract - unsafe_math               |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/config.rs                               | Smart Contract - config                    |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/mod.rs                                  | Smart Contract - states/mod                |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/operation_account.rs                    | Smart Contract - operation_account         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/oracle.rs                               | Smart Contract - oracle                    |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/personal_position.rs                    | Smart Contract - personal_position         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/pool.rs                                 | Smart Contract - pool                      |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/protocol_position.rs                    | Smart Contract - protocol_position         |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/states/tick_array.rs                           | Smart Contract - tick_array                |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/util/access_control.rs                         | Smart Contract - access_control            |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/util/mod.rs                                    | Smart Contract - util/mod                  |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/util/system.rs                                 | Smart Contract - system                    |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/util/token.rs                                  | Smart Contract - token                     |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/error.rs                                       | Smart Contract - error                     |
| https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/lib.rs                                         | Smart Contract - lib                       |

## Additional Information

Documentation and instruction for PoC can be found here:
https://github.com/raydium-io/raydium-docs/blob/master/dev-resources/raydium-clmm-dev-doc.pdf

A public testnet of Raydium's CLMM can be found at https://explorer.solana.com/address/proKtffCScMcwkFkPHFcuHawN7mWxRkhyh8PGxkTwYx However, note that testing on the public testnet is prohibited by the program rules. The public testnet is provided for reference only.
