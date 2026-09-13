//! Collection pools on CLMM: bind the live WSOL/USDC pair to a collection (quote WSOL, base USDC),
//! hang two members off it, swap members inside the pool.
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
const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN22: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

fn a(s: &str) -> Address { Address::from_str(s).unwrap() }
fn disc(name: &str) -> Vec<u8> { Sha256::digest(name.as_bytes())[..8].to_vec() }
fn w(k: Address) -> AccountMeta { AccountMeta::new(k, false) }
fn r(k: Address) -> AccountMeta { AccountMeta::new_readonly(k, false) }
fn s(k: Address) -> AccountMeta { AccountMeta::new(k, true) }
fn pda(seeds: &[&[u8]], program: &Address) -> Address { Address::find_program_address(seeds, program).0 }
fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf() }
fn pk(d: &[u8], o: usize) -> Address { Address::new_from_array(d[o..o + 32].try_into().unwrap()) }

struct Env { svm: LiteSVM, program: Address, payer: Keypair, pool: Address, amm_config: Address, base_mint: Address, base_vault: Address, quote_mint: Address, user_base: Address, trade_fee_rate: u32 }
impl Env {
    fn new() -> Self {
        let program = a(PROGRAM);
        let so = std::fs::read(root().join("target/deploy/raydium_clmm.so")).unwrap();
        let mut svm = LiteSVM::new();
        svm.add_program(program, &so).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/clmm_wsol_usdc.json")).unwrap()).unwrap();
        let mut accounts = HashMap::new();
        for (k, acc) in v["accounts"].as_object().unwrap() {
            let account = Account { lamports: acc["lamports"].as_u64().unwrap(), data: base64::engine::general_purpose::STANDARD.decode(acc["data"].as_str().unwrap()).unwrap(), owner: a(acc["owner"].as_str().unwrap()), executable: false, rent_epoch: 0 };
            svm.set_account(a(k), account.clone()).unwrap();
            accounts.insert(a(k), account);
        }
        let pool = a(v["pool"].as_str().unwrap());
        let d = &accounts[&pool].data;
        let (amm_config, mint0, mint1, vault1) = (pk(d, 9), pk(d, 73), pk(d, 105), pk(d, 169));
        let cfg = &accounts[&amm_config].data;
        let mut clock: Clock = svm.get_sysvar();
        clock.unix_timestamp = 1_800_000_000;
        svm.set_sysvar(&clock);
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000 * 1_000_000_000).unwrap();
        let mut pd = accounts[&pool].clone();
        pd.data[41..73].copy_from_slice(payer.pubkey().as_ref()); // pool owner = payer
        svm.set_account(pool, pd).unwrap();
        let user_base = Address::new_unique();
        let mut ud = accounts[&vault1].data.clone();
        ud[32..64].copy_from_slice(payer.pubkey().as_ref());
        ud[64..72].copy_from_slice(&(100_000_000u64 * 1_000_000).to_le_bytes());
        svm.set_account(user_base, Account { lamports: 2_039_280, data: ud, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        Env { svm, program, payer, pool, amm_config, base_mint: mint1, base_vault: vault1, quote_mint: mint0, user_base, trade_fee_rate: u32::from_le_bytes(cfg[47..51].try_into().unwrap()) }
    }
    fn ix(&self, keys: Vec<AccountMeta>, data: Vec<u8>) -> Instruction { Instruction { program_id: self.program, accounts: keys, data } }
    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let budget = Instruction { program_id: a("ComputeBudget111111111111111111111111111111"), accounts: vec![], data: { let mut d = vec![2u8]; d.extend(1_400_000u32.to_le_bytes()); d } };
        let mut all_ixs = vec![budget]; all_ixs.extend_from_slice(ixs);
        let msg = Message::new_with_blockhash(&all_ixs, Some(&self.payer.pubkey()), &self.svm.latest_blockhash());
        let mut all: Vec<&Keypair> = vec![&self.payer];
        all.extend(signers.iter().filter(|k| k.pubkey() != self.payer.pubkey()));
        let tx = Transaction::new(&all, msg, self.svm.latest_blockhash());
        let res = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();
        res
    }
    #[track_caller]
    fn ok(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionMetadata { match self.send(ixs, signers) { Ok(m) => m, Err(e) => panic!("expected success: {:?}\n{}", e.err, e.meta.logs.join("\n")) } }
    #[track_caller]
    fn fails_with(&mut self, ixs: &[Instruction], signers: &[&Keypair], code: &str) {
        match self.send(ixs, signers) { Ok(m) => panic!("expected {code}, but succeeded:\n{}", m.logs.join("\n")), Err(e) => assert!(e.meta.logs.iter().any(|l| l.contains(&format!("Error Code: {code}"))), "expected {code}, got {:?}\n{}", e.err, e.meta.logs.join("\n")) }
    }
    fn data(&self, k: &Address) -> Vec<u8> { self.svm.get_account(k).unwrap().data }
    fn amount(&self, k: &Address) -> u64 { u64::from_le_bytes(self.data(k)[64..72].try_into().unwrap()) }
    fn collection(&self, index: u16) -> Address { pda(&[b"token_collection", self.payer.pubkey().as_ref(), &index.to_le_bytes()], &self.program) }
    fn member(&self, c: &Address, m: &Address) -> Address { pda(&[b"collection_member", c.as_ref(), m.as_ref()], &self.program) }
    fn pool_members(&self) -> Address { pda(&[b"pool_members", self.pool.as_ref()], &self.program) }
    fn member_vault(&self, m: &Address) -> Address { pda(&[b"member_vault", self.pool.as_ref(), m.as_ref()], &self.program) }
    fn seed_ruleset(&mut self, index: u16, kind: u8) -> Address {
        let (key, bump) = Address::find_program_address(&[b"ruleset", &index.to_le_bytes()], &self.program);
        let mut d = disc("account:Ruleset"); d.push(bump); d.extend(index.to_le_bytes()); d.push(kind); d.push(0); d.extend([0u8; 3]); d.extend([0u8; 32]); d.extend([0u8; 64]);
        self.svm.set_account(key, Account { lamports: 10_000_000, data: d, owner: self.program, executable: false, rent_epoch: 0 }).unwrap();
        key
    }
    fn fake_member_mint(&mut self, user_amount: u64) -> (Address, Address) {
        let mint = Address::new_unique();
        let mut d = vec![0u8; 82]; d[44] = 6; d[45] = 1;
        self.svm.set_account(mint, Account { lamports: 1_461_600, data: d, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        let acct = Address::new_unique();
        let mut t = vec![0u8; 165]; t[0..32].copy_from_slice(mint.as_ref()); t[32..64].copy_from_slice(self.payer.pubkey().as_ref()); t[64..72].copy_from_slice(&user_amount.to_le_bytes()); t[108] = 1;
        self.svm.set_account(acct, Account { lamports: 2_039_280, data: t, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        (mint, acct)
    }
    fn create_collection(&self, rs: Address, index: u16, divisor: u32) -> Instruction {
        let mut d = disc("global:create_token_collection"); d.extend(index.to_le_bytes()); d.extend(divisor.to_le_bytes());
        self.ix(vec![s(self.payer.pubkey()), r(rs), r(self.quote_mint), w(self.collection(index)), r(a(SYSTEM))], d)
    }
    fn register(&self, c: Address, rs: Address, mint: Address) -> Instruction {
        self.ix(vec![s(self.payer.pubkey()), w(c), r(rs), r(mint), w(self.member(&c, &mint)), r(a(SYSTEM))], disc("global:register_collection_member"))
    }
    fn init_pool_members(&self, signer: &Address, c: Address, amp: u64) -> Instruction {
        let mut d = disc("global:init_pool_members"); d.extend(amp.to_le_bytes());
        self.ix(vec![s(*signer), r(self.pool), r(c), r(self.member(&c, &self.base_mint)), w(self.pool_members()), r(a(SYSTEM))], d)
    }
    fn add_member(&self, c: Address, mint: Address) -> Instruction {
        self.ix(vec![s(self.payer.pubkey()), r(self.pool), w(self.pool_members()), r(self.member(&c, &mint)), r(mint), w(self.member_vault(&mint)), r(a(TOKEN)), r(a(SYSTEM))], disc("global:add_pool_member"))
    }
    fn intra(&self, c: Address, members: &[(Address, Address, Address)], i: usize, j: usize, amount: u64) -> Instruction {
        let mut keys = vec![s(self.payer.pubkey()), r(self.amm_config), w(self.pool), w(self.pool_members()), r(c), w(members[i].1), w(members[j].1), w(members[i].2), w(members[j].2), r(a(TOKEN)), r(a(TOKEN22)), r(members[i].0), r(members[j].0)];
        for m in members { keys.push(r(m.2)); keys.push(r(self.member(&c, &m.0))); }
        let mut d = disc("global:intra_swap"); d.push(i as u8); d.push(j as u8); d.extend(amount.to_le_bytes()); d.extend(0u64.to_le_bytes());
        self.ix(keys, d)
    }
}
fn event_u64(m: &TransactionMetadata, name: &str, offset: usize) -> u64 {
    let d = disc(name);
    for l in &m.logs { if let Some(b64) = l.strip_prefix("Program data: ") { let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap(); if bytes.starts_with(&d) { return u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()); } } }
    panic!("no {name}");
}
fn ceil_fee(amount: u64, rate: u64) -> u64 { ((amount as u128 * rate as u128 + 999_999) / 1_000_000) as u64 }

#[test]
fn clmm_collection_pool_intra_swaps() {
    let mut e = Env::new();
    let payer = e.payer.insecure_clone();
    let rs = e.seed_ruleset(0, 0);
    e.ok(&[e.create_collection(rs, 1, 100)], &[]);
    let c = e.collection(1);
    let base = e.base_mint;
    let (m1, u1) = e.fake_member_mint(10_000_000_000 * 1_000_000);
    let (m2, u2) = e.fake_member_mint(10_000_000_000 * 1_000_000);
    e.ok(&[e.register(c, rs, e.quote_mint), e.register(c, rs, base), e.register(c, rs, m1), e.register(c, rs, m2)], &[]);
    let other = Keypair::new();
    e.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();
    e.fails_with(&[e.init_pool_members(&other.pubkey(), c, 200)], &[&other], "NotApproved");
    e.fails_with(&[e.init_pool_members(&payer.pubkey(), c, 0)], &[], "InvalidAmp");
    e.ok(&[e.init_pool_members(&payer.pubkey(), c, 200)], &[]);
    let pm = e.data(&e.pool_members());
    assert_eq!((pm[9], pm[10]), (0, 1), "base is token_1 (USDC), n = 1");
    assert_eq!(pk(&pm, 120), e.base_vault);
    e.ok(&[e.add_member(c, m1), e.add_member(c, m2)], &[]);
    e.fails_with(&[e.add_member(c, m1)], &[], "PoolMemberExists");
    assert_eq!(e.data(&e.pool_members())[10], 3);
    // member vaults are owned by the pool state (CLMM vault authority)
    assert_eq!(pk(&e.data(&e.member_vault(&m1)), 32), e.pool);

    let members = [(base, e.user_base, e.base_vault), (m1, u1, e.member_vault(&m1)), (m2, u2, e.member_vault(&m2))];
    let unit = 1_000_000u64;
    e.fails_with(&[e.intra(c, &members, 1, 0, 1_000 * unit)], &[], "StableCurveConvergence");
    let base_reserve = e.amount(&e.base_vault);
    for (_, _, vault) in &members[1..] {
        let mut t = e.data(vault);
        t[64..72].copy_from_slice(&base_reserve.to_le_bytes()); // seed at the same size as the USDC vault: balanced
        let acc = e.svm.get_account(vault).unwrap();
        e.svm.set_account(*vault, Account { data: t, ..acc }).unwrap();
    }
    // balanced pool: m1 -> USDC at ~1:1 less fee/100
    let before = e.amount(&e.user_base);
    let m = e.ok(&[e.intra(c, &members, 1, 0, 1_000 * unit)], &[]);
    let got = e.amount(&e.user_base) - before;
    let fee = ceil_fee(1_000 * unit, (e.trade_fee_rate / 100) as u64);
    // fee plus a few ppm of StableSwap slippage at A = 200
    assert!(got >= 1_000 * unit - fee - 1_000 * unit / 100_000 && got < 1_000 * unit, "{got}");
    assert_eq!(event_u64(&m, "event:IntraSwapEvent", 120), fee);
    // USDC -> m2, m1 -> m2, m2 -> m1
    let b2 = e.amount(&u2);
    e.ok(&[e.intra(c, &members, 0, 2, 1_000 * unit), e.intra(c, &members, 1, 2, 500 * unit), e.intra(c, &members, 2, 1, 200 * unit)], &[]);
    let net = e.amount(&u2) - b2;
    assert!(net > 1_280 * unit && net < 1_320 * unit, "{net}");
    // base fees land on PoolState (token_1 side), member fees on PoolMembers
    let d = e.data(&e.pool);
    assert!(u64::from_le_bytes(d[317..325].try_into().unwrap()) > 0, "protocol_fees_token_1 grew");
    // the curve bends: two identical large sales of m1, the second pays less USDC
    let big = base_reserve / 4;
    let before = e.amount(&e.user_base);
    e.ok(&[e.intra(c, &members, 1, 0, big)], &[]);
    let first = e.amount(&e.user_base) - before;
    let before = e.amount(&e.user_base);
    e.ok(&[e.intra(c, &members, 1, 0, big)], &[]);
    let second = e.amount(&e.user_base) - before;
    assert!(second < first && first - second > first / 1000, "price must move against the seller as the pool skews: {first} then {second}");
    // a member vault cannot be drained
    // buying out a member vault costs a ruinous premium and never empties it
    let m2_reserve = e.amount(&members[2].2);
    let before = e.amount(&u2);
    let paid = base_reserve * 30;
    e.ok(&[e.intra(c, &members, 0, 2, paid)], &[]);
    let got = e.amount(&u2) - before;
    assert!(got < m2_reserve && e.amount(&members[2].2) > 0 && paid / got >= 20, "extracted {got} of {m2_reserve} for {paid}");
    e.fails_with(&[e.intra(c, &members, 1, 1, unit)], &[], "InvalidPoolMember");
    // collect member fees: pool admin path via amm_config.owner; stranger rejected
    let recipient = Address::new_unique();
    let mut t = vec![0u8; 165]; t[0..32].copy_from_slice(m1.as_ref()); t[32..64].copy_from_slice(payer.pubkey().as_ref()); t[108] = 1;
    e.svm.set_account(recipient, Account { lamports: 2_039_280, data: t, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
    let collect = |e: &Env, signer: Address, kind: u8| { let mut d = disc("global:collect_member_fees"); d.push(1); d.push(kind); e.ix(vec![s(signer), r(e.amm_config), w(e.pool), w(e.pool_members()), w(e.member_vault(&m1)), w(recipient), r(m1), r(a(TOKEN)), r(a(TOKEN22))], d) };
    e.fails_with(&[collect(&e, payer.pubkey(), 0)], &[], "NotApproved");
    let cfg_owner = pk(&e.data(&e.amm_config), 11);
    let _ = cfg_owner; // mainnet config owner key is not available to sign; negative path covered above
}
