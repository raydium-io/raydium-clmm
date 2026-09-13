//! Token collections in CLMM: every permissionless instruction and error branch, admin gating, and
//! `rebalance_swap_v2` on live pool state.
use base64::Engine;
use litesvm::{types::FailedTransactionMetadata, types::TransactionMetadata, LiteSVM};
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{collections::HashMap, path::PathBuf, str::FromStr};

const PROGRAM: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";
const PUMP: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN22: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
const MEMO: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
const WSOL: &str = "So11111111111111111111111111111111111111112";
const RATE_ONE: u64 = 1_000_000_000;
const KIND_ANY: u8 = 0;
const KIND_PUMP: u8 = 1;
const KIND_IMMUTABLE: u8 = 2;

fn a(s: &str) -> Address {
    Address::from_str(s).unwrap()
}
fn disc(name: &str) -> Vec<u8> {
    Sha256::digest(name.as_bytes())[..8].to_vec()
}
fn w(k: Address) -> AccountMeta {
    AccountMeta::new(k, false)
}
fn r(k: Address) -> AccountMeta {
    AccountMeta::new_readonly(k, false)
}
fn s(k: Address) -> AccountMeta {
    AccountMeta::new(k, true)
}
fn pda(seeds: &[&[u8]], program: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program)
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
fn pk(d: &[u8], o: usize) -> Address {
    Address::new_from_array(d[o..o + 32].try_into().unwrap())
}
fn u128_at(d: &[u8], o: usize) -> u128 {
    u128::from_le_bytes(d[o..o + 16].try_into().unwrap())
}

struct Pool {
    key: Address,
    amm_config: Address,
    mint0: Address,
    mint1: Address,
    vault0: Address,
    vault1: Address,
    observation: Address,
    dec0: u8,
    dec1: u8,
    tick_spacing: u16,
}
struct Env {
    svm: LiteSVM,
    program: Address,
    payer: Keypair,
    pool: Pool,
    tick_arrays: Vec<Address>,
    bitmap_ext: Option<Address>,
    user_wsol: Address,
    user_usdc: Address,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
}
impl Env {
    fn new() -> Self {
        let program = a(PROGRAM);
        let so = std::fs::read(root().join("target/deploy/raydium_clmm.so")).expect("cargo build-sbf first");
        let mut svm = LiteSVM::new();
        svm.add_program(program, &so).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/clmm_wsol_usdc.json")).unwrap()).unwrap();
        let mut accounts = HashMap::new();
        for (k, acc) in v["accounts"].as_object().unwrap() {
            let account = Account {
                lamports: acc["lamports"].as_u64().unwrap(),
                data: base64::engine::general_purpose::STANDARD.decode(acc["data"].as_str().unwrap()).unwrap(),
                owner: a(acc["owner"].as_str().unwrap()),
                executable: false,
                rent_epoch: 0,
            };
            svm.set_account(a(k), account.clone()).unwrap();
            accounts.insert(a(k), account);
        }
        let pool_key = a(v["pool"].as_str().unwrap());
        let d = &accounts[&pool_key].data;
        let pool = Pool { key: pool_key, amm_config: pk(d, 9), mint0: pk(d, 73), mint1: pk(d, 105), vault0: pk(d, 137), vault1: pk(d, 169), observation: pk(d, 201), dec0: d[233], dec1: d[234], tick_spacing: u16::from_le_bytes(d[235..237].try_into().unwrap()) };
        let bitmap_ext = pda(&[b"pool_tick_array_bitmap_extension", pool_key.as_ref()], &program).0;
        let tick_arrays: Vec<Address> = accounts.iter().filter(|(k, acc)| acc.owner == program && **k != pool_key && **k != pool.amm_config && **k != pool.observation && **k != bitmap_ext).map(|(k, _)| *k).collect();
        let cfg = &accounts[&pool.amm_config].data;
        let mut clock: Clock = svm.get_sysvar();
        clock.unix_timestamp = 1_800_000_000;
        svm.set_sysvar(&clock);
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000 * 1_000_000_000).unwrap();
        // user token accounts cloned from the vault layouts with our owner
        let user_wsol = Address::new_unique();
        let mut wd = accounts[&pool.vault0].data.clone();
        wd[32..64].copy_from_slice(payer.pubkey().as_ref());
        let wsol_amt = 500u64 * 1_000_000_000;
        wd[64..72].copy_from_slice(&wsol_amt.to_le_bytes());
        svm.set_account(user_wsol, Account { lamports: 2_039_280 + wsol_amt, data: wd, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        let user_usdc = Address::new_unique();
        let mut ud = accounts[&pool.vault1].data.clone();
        ud[32..64].copy_from_slice(payer.pubkey().as_ref());
        ud[64..72].copy_from_slice(&(10_000_000u64 * 1_000_000).to_le_bytes());
        svm.set_account(user_usdc, Account { lamports: 2_039_280, data: ud, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        Env { svm, program, payer, pool, tick_arrays, bitmap_ext: accounts.contains_key(&bitmap_ext).then_some(bitmap_ext), user_wsol, user_usdc, trade_fee_rate: u32::from_le_bytes(cfg[47..51].try_into().unwrap()), protocol_fee_rate: u32::from_le_bytes(cfg[43..47].try_into().unwrap()) }
    }
    fn ix(&self, keys: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
        Instruction { program_id: self.program, accounts: keys, data }
    }
    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let mut with_budget = vec![Instruction { program_id: a("ComputeBudget111111111111111111111111111111"), accounts: vec![], data: { let mut d = vec![2u8]; d.extend(1_400_000u32.to_le_bytes()); d } }];
        with_budget.extend_from_slice(ixs);
        let msg = Message::new_with_blockhash(&with_budget, Some(&self.payer.pubkey()), &self.svm.latest_blockhash());
        let mut all: Vec<&Keypair> = vec![&self.payer];
        all.extend(signers.iter().filter(|k| k.pubkey() != self.payer.pubkey()));
        let tx = Transaction::new(&all, msg, self.svm.latest_blockhash());
        let res = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();
        res
    }
    fn ok(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionMetadata {
        match self.send(ixs, signers) {
            Ok(m) => m,
            Err(e) => panic!("expected success: {:?}\n{}", e.err, e.meta.logs.join("\n")),
        }
    }
    fn fails_with(&mut self, ixs: &[Instruction], signers: &[&Keypair], code: &str) {
        match self.send(ixs, signers) {
            Ok(m) => panic!("expected {code}, but succeeded:\n{}", m.logs.join("\n")),
            Err(e) => {
                eprintln!("{code}: {} CU", e.meta.compute_units_consumed);
                assert!(e.meta.logs.iter().any(|l| l.contains(&format!("Error Code: {code}"))), "expected {code}, got {:?}\n{}", e.err, e.meta.logs.join("\n"))
            }
        }
    }
    fn data(&self, k: &Address) -> Vec<u8> {
        self.svm.get_account(k).unwrap().data
    }
    fn ruleset(&self, index: u16) -> Address {
        pda(&[b"ruleset", &index.to_le_bytes()], &self.program).0
    }
    fn collection(&self, index: u16) -> Address {
        pda(&[b"token_collection", self.payer.pubkey().as_ref(), &index.to_le_bytes()], &self.program).0
    }
    fn member(&self, collection: &Address, mint: &Address) -> Address {
        pda(&[b"collection_member", collection.as_ref(), mint.as_ref()], &self.program).0
    }
    fn curve(&self, mint: &Address) -> Address {
        pda(&[b"bonding-curve", mint.as_ref()], &a(PUMP)).0
    }
    /// Admin-created ruleset, seeded directly (the localnet admin key is not distributed).
    fn seed_ruleset(&mut self, index: u16, kind: u8, flags: u8, program_id: Address) -> Address {
        let (key, bump) = pda(&[b"ruleset", &index.to_le_bytes()], &self.program);
        let mut d = disc("account:Ruleset");
        d.push(bump);
        d.extend(index.to_le_bytes());
        d.push(kind);
        d.push(flags);
        d.extend([0u8; 3]);
        d.extend(program_id.as_ref());
        d.extend([0u8; 64]);
        self.svm.set_account(key, Account { lamports: 10_000_000, data: d, owner: self.program, executable: false, rent_epoch: 0 }).unwrap();
        key
    }
    fn create_ruleset(&self, signer: &Address, index: u16, kind: u8, flags: u8, program_id: Address) -> Instruction {
        let mut d = disc("global:create_ruleset");
        d.extend(index.to_le_bytes());
        d.push(kind);
        d.push(flags);
        d.extend(program_id.as_ref());
        self.ix(vec![s(*signer), w(self.ruleset(index)), r(a(SYSTEM))], d)
    }
    fn update_ruleset(&self, signer: &Address, index: u16, kind: u8, flags: u8, program_id: Address) -> Instruction {
        let mut d = disc("global:update_ruleset");
        d.push(kind);
        d.push(flags);
        d.extend(program_id.as_ref());
        self.ix(vec![s(*signer), w(self.ruleset(index))], d)
    }
    fn create_collection(&self, ruleset: u16, index: u16, quote: Address, divisor: u32) -> Instruction {
        let mut d = disc("global:create_token_collection");
        d.extend(index.to_le_bytes());
        d.extend(divisor.to_le_bytes());
        self.ix(vec![s(self.payer.pubkey()), r(self.ruleset(ruleset)), r(quote), w(self.collection(index)), r(a(SYSTEM))], d)
    }
    fn update_collection(&self, signer: &Address, collection: Address, param: u8, value: u64, extra: Option<Address>) -> Instruction {
        let mut d = disc("global:update_token_collection");
        d.push(param);
        d.extend(value.to_le_bytes());
        let mut keys = vec![s(*signer), w(collection)];
        keys.extend(extra.map(r));
        self.ix(keys, d)
    }
    fn register(&self, collection: Address, ruleset: Address, mint: Address, proof: &[Address]) -> Instruction {
        let mut keys = vec![s(self.payer.pubkey()), w(collection), r(ruleset), r(mint), w(self.member(&collection, &mint)), r(a(SYSTEM))];
        keys.extend(proof.iter().map(|p| r(*p)));
        self.ix(keys, disc("global:register_collection_member"))
    }
    fn set_rate(&self, signer: &Address, collection: Address, member: Address, rate: u64) -> Instruction {
        let mut d = disc("global:set_collection_member_rate");
        d.extend(rate.to_le_bytes());
        self.ix(vec![s(*signer), r(collection), w(member)], d)
    }
    fn swap_keys(&self, zero_for_one: bool) -> Vec<AccountMeta> {
        let p = &self.pool;
        let (ua, ub, va, vb, ma, mb) = if zero_for_one { (self.user_wsol, self.user_usdc, p.vault0, p.vault1, p.mint0, p.mint1) } else { (self.user_usdc, self.user_wsol, p.vault1, p.vault0, p.mint1, p.mint0) };
        vec![s(self.payer.pubkey()), r(p.amm_config), w(p.key), w(ua), w(ub), w(va), w(vb), w(p.observation), r(a(TOKEN)), r(a(TOKEN22)), r(a(MEMO)), r(ma), r(mb)]
    }
    fn args(amount: u64) -> Vec<u8> {
        let mut d = amount.to_le_bytes().to_vec();
        d.extend(0u64.to_le_bytes());
        d.extend(0u128.to_le_bytes());
        d.push(1);
        d
    }
    fn swap_v2(&self, zero_for_one: bool, amount: u64) -> Instruction {
        let mut keys = self.swap_keys(zero_for_one);
        keys.extend(self.remaining_for(zero_for_one));
        let mut d = disc("global:swap_v2");
        d.extend(Self::args(amount));
        self.ix(keys, d)
    }
    fn rebalance_v2(&self, zero_for_one: bool, amount: u64, collection: Address) -> Instruction {
        let p = &self.pool;
        let (im, om) = if zero_for_one { (p.mint0, p.mint1) } else { (p.mint1, p.mint0) };
        let mut keys = self.swap_keys(zero_for_one);
        keys.extend([r(collection), r(self.member(&collection, &im)), r(self.member(&collection, &om))]);
        keys.extend(self.remaining_for(zero_for_one));
        let mut d = disc("global:rebalance_swap_v2");
        d.extend(Self::args(amount));
        self.ix(keys, d)
    }
    /// bitmap extension (if any) then tick arrays starting at the one containing the current tick, in trade direction.
    fn remaining_for(&self, zero_for_one: bool) -> Vec<AccountMeta> {
        let tick = i32::from_le_bytes(self.data(&self.pool.key)[269..273].try_into().unwrap());
        let per = self.pool.tick_spacing as i32 * 60;
        let start = tick.div_euclid(per) * per;
        let mut arrays: Vec<(i32, Address)> = self.tick_arrays.iter().map(|k| (i32::from_le_bytes(self.data(k)[40..44].try_into().unwrap()), *k)).collect();
        arrays.retain(|(si, _)| if zero_for_one { *si <= start } else { *si >= start });
        arrays.sort();
        if zero_for_one {
            arrays.reverse();
        }
        let mut v: Vec<AccountMeta> = self.bitmap_ext.iter().map(|k| w(*k)).collect();
        v.extend(arrays.into_iter().map(|(_, k)| w(k)));
        v
    }
    fn sqrt_price(&self) -> u128 {
        u128_at(&self.data(&self.pool.key), 253)
    }
    fn protocol_fees(&self) -> (u64, u64) {
        let d = self.data(&self.pool.key);
        (u64::from_le_bytes(d[309..317].try_into().unwrap()), u64::from_le_bytes(d[317..325].try_into().unwrap()))
    }
    /// USDC rate that makes the collection target equal the pool's current price.
    fn balanced_usdc_rate(&self) -> u64 {
        let price_raw = (self.sqrt_price() as f64 / 2f64.powi(64)).powi(2);
        (1e9 * 10f64.powi(self.pool.dec1 as i32 - self.pool.dec0 as i32) / price_raw).round() as u64
    }
    fn fake_mint(&mut self, mint_authority: bool) -> Address {
        let mint = Address::new_unique();
        let mut d = vec![0u8; 82];
        if mint_authority {
            d[0..4].copy_from_slice(&1u32.to_le_bytes());
            d[4..36].copy_from_slice(Address::new_unique().as_ref());
        }
        d[44] = 6;
        d[45] = 1;
        self.svm.set_account(mint, Account { lamports: 1_461_600, data: d, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        mint
    }
    /// pump.fun BondingCurve: disc | 5 x u64 | complete | creator | is_mayhem | ... (125 bytes)
    fn fake_curve(&mut self, mint: &Address, complete: u8, mayhem: u8, owner: Address, bad_disc: bool) -> Address {
        let mut d = disc("account:BondingCurve");
        if bad_disc {
            d[0] ^= 0xff;
        }
        d.extend([0u8; 40]);
        d.push(complete);
        d.extend([0u8; 32]);
        d.push(mayhem);
        d.extend([0u8; 43]);
        let k = self.curve(mint);
        self.svm.set_account(k, Account { lamports: 2_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
        k
    }
}
/// Effective LP fee rate (ppm of input) recovered from the protocol fee delta.
fn lp_fee_ppm(protocol_fee_delta: u64, protocol_fee_rate: u32, input: u64) -> f64 {
    protocol_fee_delta as f64 * 1e6 / protocol_fee_rate as f64 * 1e6 / input as f64
}

#[test]
fn admin_gating_negative() {
    let mut e = Env::new();
    let payer = e.payer.insecure_clone();
    e.fails_with(&[e.create_ruleset(&payer.pubkey(), 9, KIND_ANY, 0, Address::default())], &[], "NotApproved");
    e.seed_ruleset(9, KIND_ANY, 0, Address::default());
    e.fails_with(&[e.update_ruleset(&payer.pubkey(), 9, KIND_ANY, 0, Address::default())], &[], "NotApproved");
}

#[test]
fn collections_and_members() {
    let mut e = Env::new();
    let payer = e.payer.insecure_clone();
    let rs_any = e.seed_ruleset(0, KIND_ANY, 0, Address::default());
    let rs_pump = e.seed_ruleset(1, KIND_PUMP, 0, a(PUMP));
    let rs_imm = e.seed_ruleset(2, KIND_IMMUTABLE, 0, Address::default());
    let rs_mayhem = e.seed_ruleset(3, KIND_PUMP, 1, a(PUMP));
    let rs_complete = e.seed_ruleset(4, KIND_PUMP, 2, a(PUMP));
    e.fails_with(&[e.create_collection(0, 0, a(WSOL), 0)], &[], "InvalidUpdateConfigFlag");
    e.ok(&[e.create_collection(0, 0, a(WSOL), 100), e.create_collection(1, 1, a(WSOL), 100), e.create_collection(2, 2, a(WSOL), 100), e.create_collection(3, 3, a(WSOL), 100), e.create_collection(4, 4, a(WSOL), 100)], &[]);
    let (c_any, c_pump, c_imm, c_mayhem, c_complete) = (e.collection(0), e.collection(1), e.collection(2), e.collection(3), e.collection(4));
    let d = e.data(&c_pump);
    assert_eq!((pk(&d, 16), pk(&d, 48), pk(&d, 80), u32::from_le_bytes(d[112..116].try_into().unwrap())), (payer.pubkey(), rs_pump, a(WSOL), 100));
    // update collection
    e.ok(&[e.update_collection(&payer.pubkey(), c_any, 0, 1000, None)], &[]);
    assert_eq!(u32::from_le_bytes(e.data(&c_any)[112..116].try_into().unwrap()), 1000);
    e.fails_with(&[e.update_collection(&payer.pubkey(), c_any, 0, 0, None)], &[], "InvalidUpdateConfigFlag");
    e.fails_with(&[e.update_collection(&payer.pubkey(), c_any, 7, 0, None)], &[], "InvalidUpdateConfigFlag");
    e.fails_with(&[e.update_collection(&payer.pubkey(), c_any, 1, 0, None)], &[], "InvalidUpdateConfigFlag");
    let other = Keypair::new();
    e.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();
    e.fails_with(&[e.update_collection(&other.pubkey(), c_any, 0, 5, None)], &[&other], "NotApproved");
    e.ok(&[e.update_collection(&payer.pubkey(), c_any, 1, 0, Some(other.pubkey()))], &[]);
    e.fails_with(&[e.update_collection(&payer.pubkey(), c_any, 0, 5, None)], &[], "NotApproved");
    e.ok(&[e.update_collection(&other.pubkey(), c_any, 0, 5, None)], &[&other]);
    // membership: quote bypass, Any, Immutable, pump rule branches
    let usdc = e.pool.mint1;
    e.ok(&[e.register(c_pump, rs_pump, a(WSOL), &[])], &[]);
    e.fails_with(&[e.register(c_pump, rs_pump, usdc, &[])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c_pump, rs_any, usdc, &[])], &[], "InvalidUpdateConfigFlag");
    let with_auth = e.fake_mint(true);
    let immutable = e.fake_mint(false);
    e.ok(&[e.register(c_any, rs_any, with_auth, &[]), e.register(c_imm, rs_imm, immutable, &[])], &[]);
    e.fails_with(&[e.register(c_imm, rs_imm, with_auth, &[])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c_imm, rs_imm, usdc, &[])], &[], "RuleCheckFailed"); // USDC has a mint authority
    assert!(e.send(&[e.register(c_any, rs_any, with_auth, &[])], &[]).is_err(), "duplicate");
    let m = e.fake_mint(false);
    e.fake_curve(&m, 1, 0, a(PUMP), false);
    e.ok(&[e.register(c_pump, rs_pump, m, &[e.curve(&m)])], &[]);
    let d = e.data(&e.member(&c_pump, &m));
    assert_eq!((pk(&d, 16), pk(&d, 48), u64::from_le_bytes(d[80..88].try_into().unwrap())), (c_pump, m, RATE_ONE));
    assert_eq!(u32::from_le_bytes(e.data(&c_pump)[116..120].try_into().unwrap()), 2);
    let m2 = e.fake_mint(false);
    e.fake_curve(&m2, 1, 1, a(PUMP), false); // mayhem
    e.fails_with(&[e.register(c_pump, rs_pump, m2, &[e.curve(&m2)])], &[], "RuleCheckFailed");
    e.ok(&[e.register(c_mayhem, rs_mayhem, m2, &[e.curve(&m2)])], &[]);
    let m3 = e.fake_mint(false);
    e.fake_curve(&m3, 0, 0, a(PUMP), false); // incomplete
    e.ok(&[e.register(c_pump, rs_pump, m3, &[e.curve(&m3)])], &[]);
    e.fails_with(&[e.register(c_complete, rs_complete, m3, &[e.curve(&m3)])], &[], "RuleCheckFailed");
    let m4 = e.fake_mint(false);
    e.fake_curve(&m4, 1, 0, a(SYSTEM), false); // wrong owner
    e.fails_with(&[e.register(c_pump, rs_pump, m4, &[e.curve(&m4)])], &[], "RuleCheckFailed");
    e.fake_curve(&m4, 1, 0, a(PUMP), true); // wrong discriminator
    e.fails_with(&[e.register(c_pump, rs_pump, m4, &[e.curve(&m4)])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c_pump, rs_pump, m4, &[e.curve(&m)])], &[], "RuleCheckFailed"); // someone else's curve
    e.fails_with(&[e.register(c_pump, rs_pump, m4, &[])], &[], "RuleCheckFailed"); // no proof
    // rates
    let mem = e.member(&c_pump, &m);
    e.ok(&[e.set_rate(&payer.pubkey(), c_pump, mem, 7)], &[]);
    assert_eq!(u64::from_le_bytes(e.data(&mem)[80..88].try_into().unwrap()), 7);
    e.fails_with(&[e.set_rate(&payer.pubkey(), c_pump, mem, 0)], &[], "InvalidUpdateConfigFlag");
    e.fails_with(&[e.set_rate(&other.pubkey(), c_pump, mem, 7)], &[&other], "NotApproved");
    e.fails_with(&[e.set_rate(&payer.pubkey(), c_mayhem, mem, 7)], &[], "InvalidCollectionMember");
}

#[test]
fn rebalance_swap_v2_gating_and_fee() {
    let mut e = Env::new();
    let payer = e.payer.insecure_clone();
    let rs = e.seed_ruleset(0, KIND_ANY, 0, Address::default());
    e.ok(&[e.create_collection(0, 0, a(WSOL), 100), e.create_collection(0, 1, a(WSOL), 500)], &[]);
    let (c, c500) = (e.collection(0), e.collection(1));
    let usdc = e.pool.mint1;
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, usdc, &[]), e.register(c500, rs, a(WSOL), &[]), e.register(c500, rs, usdc, &[])], &[]);
    let rate = e.balanced_usdc_rate();
    e.ok(&[e.set_rate(&payer.pubkey(), c, e.member(&c, &usdc), rate), e.set_rate(&payer.pubkey(), c500, e.member(&c500, &usdc), rate)], &[]);
    let sol = 1_000_000_000u64;
    let usdc_unit = 1_000_000u64;

    // balanced: neither direction is a rebalance
    e.fails_with(&[e.rebalance_v2(true, sol, c)], &[], "NotRebalancing");
    e.fails_with(&[e.rebalance_v2(false, 100 * usdc_unit, c)], &[], "NotRebalancing");

    // standard swap_v2 unchanged: full fee, price moves down
    let p0 = e.sqrt_price();
    let (pf0, _) = e.protocol_fees();
    e.ok(&[e.swap_v2(true, 30 * sol)], &[]);
    let p1 = e.sqrt_price();
    assert!(p1 < p0);
    let (pf0b, _) = e.protocol_fees();
    let ppm = lp_fee_ppm(pf0b - pf0, e.protocol_fee_rate, 30 * sol);
    assert!((ppm - e.trade_fee_rate as f64).abs() < 1.0, "standard fee {ppm} ppm vs {}", e.trade_fee_rate);

    // same direction rejected; toward target accepted at fee/100 and fee/500
    e.fails_with(&[e.rebalance_v2(true, sol, c)], &[], "NotRebalancing");
    let (_, pf1) = e.protocol_fees();
    let leg = 1_000 * usdc_unit;
    let m = e.ok(&[e.rebalance_v2(false, leg, c)], &[]);
    eprintln!("rebalance_swap_v2 CU: {}", m.compute_units_consumed);
    assert!(m.compute_units_consumed < 400_000, "CU {}", m.compute_units_consumed);
    let p2 = e.sqrt_price();
    assert!(p2 > p1 && p2 < p0, "moved toward target without crossing");
    let (_, pf1b) = e.protocol_fees();
    let ppm = lp_fee_ppm(pf1b - pf1, e.protocol_fee_rate, leg);
    assert!((ppm - (e.trade_fee_rate / 100) as f64).abs() < 1.0, "rebalance fee {ppm} ppm");
    e.ok(&[e.rebalance_v2(false, leg / 2, c500)], &[]);
    let (_, pf1c) = e.protocol_fees();
    let ppm = lp_fee_ppm(pf1c - pf1b, e.protocol_fee_rate, leg / 2);
    assert!(ppm <= 1.0 + (e.trade_fee_rate / 500) as f64, "fee/500 -> {ppm} ppm");
    // divisor larger than the fee rate floors at 1 ppm and still executes
    e.ok(&[e.update_collection(&payer.pubkey(), c, 0, 1_000_000, None)], &[]);
    let (_, pfx) = e.protocol_fees();
    e.ok(&[e.rebalance_v2(false, leg / 4, c)], &[]);
    let (_, pfy) = e.protocol_fees();
    assert!(pfy >= pfx, "fee never negative");
    // overshoot past the target rejected
    e.fails_with(&[e.rebalance_v2(false, 100_000 * usdc_unit, c)], &[], "NotRebalancing");
    // wrong member for the collection
    let mut bad = e.rebalance_v2(false, leg, c);
    bad.accounts[14] = r(e.member(&c500, &usdc));
    e.fails_with(&[bad], &[], "ConstraintSeeds");
    // collection missing a member
    e.ok(&[e.create_collection(0, 2, a(WSOL), 100)], &[]);
    let c2 = e.collection(2);
    e.ok(&[e.register(c2, rs, a(WSOL), &[])], &[]);
    e.fails_with(&[e.rebalance_v2(false, leg, c2)], &[], "AccountNotInitialized");
}

// ---------------------------------------------------------------- LST rule: stake pool proof and rate sync

const STAKE_POOL_PROGRAM: &str = "SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy";
fn fake_stake_pool(e: &mut Env, mint: &Address, total_lamports: u64, supply: u64, owner: Address, account_type: u8) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 611];
    d[0] = account_type;
    d[162..194].copy_from_slice(mint.as_ref());
    d[258..266].copy_from_slice(&total_lamports.to_le_bytes());
    d[266..274].copy_from_slice(&supply.to_le_bytes());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}

#[test]
fn lst_rule_and_rate_sync() {
    let mut e = Env::new();
    let sp = a(STAKE_POOL_PROGRAM);
    let rs = e.seed_ruleset(5, 3, 0, sp);
    e.ok(&[e.create_collection(5, 5, a(WSOL), 100)], &[]);
    let c = e.collection(5);
    let lst = e.fake_mint(true);
    let pool = fake_stake_pool(&mut e, &lst, 1_150_000 * 1_000_000_000, 1_000_000 * 1_000_000_000, sp, 1);
    e.fails_with(&[e.register(c, rs, lst, &[])], &[], "RuleCheckFailed");
    let wrong_owner = fake_stake_pool(&mut e, &lst, 1, 1, a(SYSTEM), 1);
    e.fails_with(&[e.register(c, rs, lst, &[wrong_owner])], &[], "RuleCheckFailed");
    let wrong_type = fake_stake_pool(&mut e, &lst, 1, 1, sp, 2);
    e.fails_with(&[e.register(c, rs, lst, &[wrong_type])], &[], "RuleCheckFailed");
    let other_mint = e.fake_mint(true);
    let other_pool = fake_stake_pool(&mut e, &other_mint, 1, 1, sp, 1);
    e.fails_with(&[e.register(c, rs, lst, &[other_pool])], &[], "RuleCheckFailed");
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, lst, &[pool])], &[]);
    let member = e.member(&c, &lst);
    let rate = |e: &Env| u64::from_le_bytes(e.data(&member)[80..88].try_into().unwrap());
    assert_eq!(rate(&e), 1_150_000_000);
    let mut pd = e.svm.get_account(&pool).unwrap();
    pd.data[258..266].copy_from_slice(&(1_200_000u64 * 1_000_000_000).to_le_bytes());
    e.svm.set_account(pool, pd).unwrap();
    let sync = |e: &Env, member: Address, proof: Address| e.ix(vec![r(c), r(rs), w(member), r(proof)], disc("global:sync_member_rate"));
    e.ok(&[sync(&e, member, pool)], &[]);
    assert_eq!(rate(&e), 1_200_000_000);
    e.fails_with(&[sync(&e, member, other_pool)], &[], "RuleCheckFailed");
    e.fails_with(&[sync(&e, e.member(&c, &a(WSOL)), pool)], &[], "InvalidCollectionMember");
    let rs_any = e.seed_ruleset(6, KIND_ANY, 0, Address::default());
    e.ok(&[e.create_collection(6, 6, a(WSOL), 100)], &[]);
    let c6 = e.collection(6);
    e.ok(&[e.register(c6, rs_any, lst, &[])], &[]);
    e.fails_with(&[e.ix(vec![r(c6), r(rs_any), w(e.member(&c6, &lst)), r(pool)], disc("global:sync_member_rate"))], &[], "InvalidRuleKind");
    let fresh = e.fake_mint(true);
    let empty = fake_stake_pool(&mut e, &fresh, 0, 0, sp, 1);
    e.ok(&[e.register(c, rs, fresh, &[empty])], &[]);
    assert_eq!(u64::from_le_bytes(e.data(&e.member(&c, &fresh))[80..88].try_into().unwrap()), RATE_ONE);
}

// ---------------------------------------------------------------- LaunchpadDbc rule: Meteora DBC virtual pool + config proof

const DBC_PROGRAM: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
/// VirtualPool: disc | PoolState { volatility 64 | config @72 | creator @104 | base_mint @136 | ... } (424 bytes)
fn fake_virtual_pool(e: &mut Env, config: &Address, base_mint: &Address, owner: Address, bad_disc: bool) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 424];
    d[..8].copy_from_slice(&[213, 224, 5, 209, 98, 69, 119, 92]);
    if bad_disc { d[0] ^= 0xff; }
    d[72..104].copy_from_slice(config.as_ref());
    d[136..168].copy_from_slice(base_mint.as_ref());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}
/// PoolConfig: disc | quote_mint @8 | fee_claimer @40 | leftover_receiver @72 | ... (1048 bytes)
fn fake_pool_config(e: &mut Env, fee_claimer: &Address, owner: Address) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 1048];
    d[..8].copy_from_slice(&[26, 108, 14, 123, 116, 230, 129, 43]);
    d[8..40].copy_from_slice(a(WSOL).as_ref());
    d[40..72].copy_from_slice(fee_claimer.as_ref());
    d[72..104].copy_from_slice(fee_claimer.as_ref());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}

#[test]
fn launchpad_dbc_rule() {
    let mut e = Env::new();
    let partner = Address::new_unique(); // our launchpad partner PDA, the ruleset's program_id field
    let dbc = a(DBC_PROGRAM);
    let rs = e.seed_ruleset(7, 4, 0, partner);
    e.ok(&[e.create_collection(7, 7, a(WSOL), 100)], &[]);
    let c = e.collection(7);
    let meme = e.fake_mint(false);
    let our_cfg = fake_pool_config(&mut e, &partner, dbc);
    let vp = fake_virtual_pool(&mut e, &our_cfg, &meme, dbc, false);
    // negatives: missing proof, only one account, wrong owner on either, bad discriminator, config mismatch,
    // wrong mint, someone else's config (different fee claimer)
    e.fails_with(&[e.register(c, rs, meme, &[])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c, rs, meme, &[vp])], &[], "RuleCheckFailed");
    let vp_sys = fake_virtual_pool(&mut e, &our_cfg, &meme, a(SYSTEM), false);
    e.fails_with(&[e.register(c, rs, meme, &[vp_sys, our_cfg])], &[], "RuleCheckFailed");
    let cfg_sys = fake_pool_config(&mut e, &partner, a(SYSTEM));
    let vp2 = fake_virtual_pool(&mut e, &cfg_sys, &meme, dbc, false);
    e.fails_with(&[e.register(c, rs, meme, &[vp2, cfg_sys])], &[], "RuleCheckFailed");
    let vp_bad = fake_virtual_pool(&mut e, &our_cfg, &meme, dbc, true);
    e.fails_with(&[e.register(c, rs, meme, &[vp_bad, our_cfg])], &[], "RuleCheckFailed");
    let other_cfg = fake_pool_config(&mut e, &partner, dbc);
    e.fails_with(&[e.register(c, rs, meme, &[vp, other_cfg])], &[], "RuleCheckFailed"); // pool points at our_cfg, not other_cfg
    let other_mint = e.fake_mint(false);
    e.fails_with(&[e.register(c, rs, other_mint, &[vp, our_cfg])], &[], "RuleCheckFailed");
    let foreign_cfg = fake_pool_config(&mut e, &Address::new_unique(), dbc);
    let vp_foreign = fake_virtual_pool(&mut e, &foreign_cfg, &meme, dbc, false);
    e.fails_with(&[e.register(c, rs, meme, &[vp_foreign, foreign_cfg])], &[], "RuleCheckFailed");
    // the real thing
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, meme, &[vp, our_cfg])], &[]);
    let d = e.data(&e.member(&c, &meme));
    assert_eq!((pk(&d, 16), pk(&d, 48), u64::from_le_bytes(d[80..88].try_into().unwrap())), (c, meme, RATE_ONE));
}
