# Collection rebalance swap for CLMM

## the pitch

companion to the CP-Swap token collections PR. same framing: a stable is a stable, an lst is an lst, a meme is a meme, more or less. Curve made the pool the cheapest place to keep near-substitutes near each other; Solidly made the fee flow pay whoever keeps the system honest. this PR gives CLMM pools the same intra-collection rebalance path, reusing the collection registry that lives in CP-Swap so Raydium has one definition of "what a collection is" across both AMMs.

## what changes

Additive. No existing instruction or layout changes.

- `states/collection.rs`: read-only views of CP-Swap `TokenCollection` and `CollectionMember` accounts (owner + Anchor discriminator + field checks), and `target_sqrt_price_x64(rate_0, rate_1, decimals_0, decimals_1)`: the balanced price implied by member rates.
- `exact_internal_v2` now delegates to `exact_internal_v2_with_fee(..., trade_fee_rate)`; the existing path passes the config rate, so behaviour is unchanged. The fee is threaded by cloning `AmmConfig` with the effective rate, so `swap_internal`'s signature and its tests are untouched.
- `rebalance_swap_v2`: `swap_v2` accounts + `collection`, `input_member`, `output_member`. LP fee is `trade_fee_rate / collection.rebalance_fee_divisor`. Accepted only if the trade direction points at the target price and `|sqrt_price - target|` strictly decreases.

## verified

- `cargo test collection`: target price for a pegged pair is exactly `2^64`; SOL(9)/USDC(6) at 150 gives raw price 0.15.
- builds with `cargo build-sbf`.
- fork demo against the live WSOL/USDC CLMM pool: see `scripts/raydium-clmm-fork-demo.mjs` in the companion repo.

## open questions

- reading CP-Swap accounts from CLMM is a cross-program dependency; the alternative is duplicating the registry here. we chose one source of truth.
- rate semantics and oracle feeding, same as the CP-Swap PR.
