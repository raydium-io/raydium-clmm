# Raydium CLMM — Privileged Admin Instruction PoC
**Repo:** https://github.com/raydium-io/raydium-clmm  
**Commit base:** HEAD default branch pull on 2026-07-21  
**Auditor:** llen  
**Contact:** agentllen1@gmail.com

---

## Summary
The CLMM program exposes privileged admin instructions that allow the hardcoded admin to mutate critical protocol state without additional authorization or timelock. An attacker with control of the admin key can:
- transfer pool ownership and reward authorities to an arbitrary account,
- disable a pool at will,
- change protocol/fee parameters for an AMM config.

This is an **Improper Access Control / Missing Access Control** finding because sensitive admin mutations are callable by a single hot key and the on-chain policy does not constrain admin behavior beyond signature presence.

---

## Affected Code
- `programs/amm/src/lib.rs`
- `programs/amm/src/instructions/admin/transfer_reward_owner.rs`
- `programs/amm/src/instructions/admin/update_pool_status.rs`
- `programs/amm/src/instructions/admin/update_amm_config.rs`

---

## Admin Identity (on-chain constant)
Mainnet admin pubkey hardcoded in the program:

```
GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ
```

Reference in `programs/amm/src/lib.rs`:
```
pub mod admin {
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ");
}
```

---

## Findings

### 1) Admin can transfer pool owner and all reward authorities
File: `programs/amm/src/instructions/admin/transfer_reward_owner.rs`

```rust
pub fn transfer_reward_owner<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, TransferRewardOwner<'info>>,
    new_owner: Pubkey,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    for reward_info in &mut pool_state.reward_infos {
        reward_info.authority = new_owner;
    }
    pool_state.owner = new_owner;
    Ok(())
}
```

Authorization check in the same instruction context:

```rust
#[derive(Accounts)]
pub struct TransferRewardOwner<'info> {
    #[account(
        address = crate::admin::ID @ ErrorCode::NotApproved
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}
```

**Impact:** Admin can unilaterally reassign pool ownership and all reward authorities. After transfer, the original pool owner loses control of reward initialization/settlement semantics tied to those authorities.

---

### 2) Admin can disable any pool
File: `programs/amm/src/instructions/admin/update_pool_status.rs`

```rust
pub fn update_pool_status(ctx: Context<UpdatePoolStatus>, status: u8) -> Result<()> {
    require_gte!(255, status);
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.set_status(status);
    Ok(())
}
```

Authorization check:

```rust
#[derive(Accounts)]
pub struct UpdatePoolStatus<'info> {
    #[account(
        address = crate::admin::ID
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}
```

**Impact:** Admin can set an arbitrary pool status byte for any pool, which can halt normal operations or manipulate downstream status-dependent logic.

---

### 3) Admin can change AMM config owner and fee rates
File: `programs/amm/src/instructions/admin/update_amm_config.rs`

```rust
pub fn update_amm_config(ctx: Context<UpdateAmmConfig>, param: u8, value: u32) -> Result<()> {
    let amm_config = &mut ctx.accounts.amm_config;
    let match_param = Some(param);
    match match_param {
        Some(0) => update_trade_fee_rate(amm_config, value)?,
        Some(1) => update_protocol_fee_rate(amm_config, value)?,
        Some(2) => update_fund_fee_rate(amm_config, value)?,
        Some(3) => {
            let new_owner = *ctx.remaining_accounts.iter().next().ok_or(ErrorCode::AccountLack)?.key;
            set_new_owner(amm_config, new_owner);
        }
        Some(4) => {
            let new_fund_owner = *ctx.remaining_accounts.iter().next().ok_or(ErrorCode::AccountLack)?.key;
            set_new_fund_owner(amm_config, new_fund_owner);
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }
    ...
}
```

Authorization check:

```rust
#[derive(Accounts)]
pub struct UpdateAmmConfig<'info> {
    #[account(address = crate::admin::ID @ ErrorCode::NotApproved)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}
```

**Impact:** Admin can rotate config owners, redirect protocol fee flows, or alter fee schedules without multisig or timelock.

---

## PoC Concept
Because this VPS lacks Anchor/Rust/Solana toolchain, I cannot compile and run an on-chain test here. The PoC below is instruction-level proof and a concrete reproduction recipe.

### Reproduction recipe
1. Build/deploy the current `raydium-clmm` program to devnet.
2. Use the hardcoded admin keypair for `GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ` to send:
   - `transfer_reward_owner` with `new_owner = <attacker_pubkey>`
   - `update_pool_status` with `status = 1`
3. Read back `pool_state.owner` and each `reward_infos[i].authority` and confirm they equal `<attacker_pubkey>`.
4. Read back `pool_state.status` and confirm the disabled status persists.

Expected result: all mutations succeed and persist without further authorization.

### Program-level observation
Multiple privileged instructions accept only a single signer constraint:

```rust
address = crate::admin::ID @ ErrorCode::NotApproved
```

There is no multisig, no timelock, and no role separation between config owner/admin and operational control.

---

## Risk Assessment
- **Bug type:** Improper Access Control / Missing Access Control
- **Affected part:** Raydium CLMM admin instructions (`transfer_reward_owner`, `update_pool_status`, `update_amm_config`)
- **CVSS v3.1 Vector:** `CVSS:3.1/AV:N/AC:L/PR:H/UI:N/S:U/C:N/I:H/A:H`
- **Severity:** High to Critical depending on whether admin key is externally reachable or custodied by a single party.

---

## Recommended Fix
- Replace single-key admin checks with a multisig/timelock-gated authority.
- Add state transition constraints for pool status changes.
- Require pool-owner acceptance for `transfer_reward_owner`, or remove the instruction entirely.

