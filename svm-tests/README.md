# SVM tests for token collections (CLMM)

Integration tests for the collection module and `rebalance_swap_v2`, run against the compiled program with
[LiteSVM] on real mainnet state (`fixtures/clmm_wsol_usdc.json`: the live WSOL/USDC pool
`3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv`, its config, vaults, observation, bitmap extension and the
tick arrays around the current tick).

The localnet admin key is not distributed, so admin-gated `create_ruleset` / `update_ruleset` are covered
negatively (non-admin rejected) and rulesets are seeded directly into the SVM for the permissionless paths.

```sh
cargo build-sbf --manifest-path programs/amm/Cargo.toml --features localnet
cargo test --manifest-path svm-tests/Cargo.toml
```

[LiteSVM]: https://github.com/LiteSVM/litesvm
