//! Integration tests: the compiled SBF program (target/deploy/lmsr_sol.so)
//! running inside LiteSVM with the real SPL Token program.
//! Build first: `cargo build-sbf --manifest-path program/Cargo.toml`.
use litesvm::LiteSVM;
use lmsr_math::{lmsr_max_loss, to_units_up, WAD};
use lmsr_sol::{state::Market, state::Position, LmsrInstruction};
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const TOKEN: Address = Address::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const SYSTEM: Address = Address::from_str_const("11111111111111111111111111111111");
const CB: Address = Address::from_str_const("ComputeBudget111111111111111111111111111111");
const DECIMALS: u8 = 6;
const UNIT: u64 = 1_000_000; // one whole token in base units

fn a(p: solana_program::pubkey::Pubkey) -> Address {
    Address::new_from_array(p.to_bytes())
}
fn p(a: &Address) -> solana_program::pubkey::Pubkey {
    solana_program::pubkey::Pubkey::new_from_array(a.to_bytes())
}

struct Env {
    svm: LiteSVM,
    program: Address,
    mint: Address,
}

fn mint_data(decimals: u8) -> Vec<u8> {
    let mut d = vec![0u8; 82];
    d[36..44].copy_from_slice(&u64::MAX.to_le_bytes()[..8]); // supply (irrelevant)
    d[44] = decimals;
    d[45] = 1;
    d
}

fn token_account_data(mint: &Address, owner: &Address, amount: u64) -> Vec<u8> {
    let mut d = vec![0u8; 165];
    d[0..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = 1; // AccountState::Initialized
    d
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let program = Address::new_unique();
        let so = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/deploy/lmsr_sol.so");
        svm.add_program_from_file(program, so).expect("build the program with cargo build-sbf first");
        let mint = Address::new_unique();
        let rent = svm.minimum_balance_for_rent_exemption(82);
        svm.set_account(mint, Account { lamports: rent, data: mint_data(DECIMALS), owner: TOKEN, executable: false, rent_epoch: 0 })
            .unwrap();
        Env { svm, program, mint }
    }

    fn user(&mut self, tokens: u64) -> (Keypair, Address) {
        let kp = Keypair::new();
        self.svm.airdrop(&kp.pubkey(), 10_000_000_000).unwrap();
        let ta = Address::new_unique();
        let rent = self.svm.minimum_balance_for_rent_exemption(165);
        self.svm
            .set_account(ta, Account { lamports: rent, data: token_account_data(&self.mint, &kp.pubkey(), tokens), owner: TOKEN, executable: false, rent_epoch: 0 })
            .unwrap();
        (kp, ta)
    }

    fn balance(&self, ta: &Address) -> u64 {
        let d = self.svm.get_account(ta).unwrap().data;
        u64::from_le_bytes(d[64..72].try_into().unwrap())
    }

    fn send(&mut self, payer: &Keypair, ix: Instruction) -> Result<u64, String> {
        let cu = Instruction { program_id: CB, accounts: vec![], data: { let mut d = vec![2u8]; d.extend_from_slice(&1_400_000u32.to_le_bytes()); d } };
        self.svm.expire_blockhash();
        let msg = Message::new(&[cu, ix], Some(&payer.pubkey()));
        let tx = Transaction::new(&[payer], msg, self.svm.latest_blockhash());
        self.svm.send_transaction(tx).map(|m| m.compute_units_consumed).map_err(|e| format!("{:?} {:?}", e.err, e.meta.logs))
    }

    fn pdas(&self, authority: &Address, id: u64) -> (Address, Address) {
        let (m, _) = lmsr_sol::market_pda(&p(&self.program), &p(authority), id);
        let (v, _) = lmsr_sol::vault_pda(&p(&self.program), &m);
        (a(m), a(v))
    }

    fn pos(&self, market: &Address, owner: &Address) -> Address {
        a(lmsr_sol::position_pda(&p(&self.program), &p(market), &p(owner)).0)
    }

    fn create(&mut self, auth: &Keypair, auth_ta: &Address, id: u64, n: u8, b_wad: u128) -> Result<(Address, Address), String> {
        let (m, v) = self.pdas(&auth.pubkey(), id);
        let ix = Instruction {
            program_id: self.program,
            accounts: vec![
                AccountMeta::new(auth.pubkey(), true),
                AccountMeta::new(m, false),
                AccountMeta::new(v, false),
                AccountMeta::new_readonly(self.mint, false),
                AccountMeta::new(*auth_ta, false),
                AccountMeta::new_readonly(TOKEN, false),
                AccountMeta::new_readonly(SYSTEM, false),
            ],
            data: LmsrInstruction::CreateMarket { market_id: id, n, b_wad }.pack(),
        };
        self.send(auth, ix).map(|_| (m, v))
    }

    fn trade_ix(&self, trader: &Keypair, ta: &Address, m: &Address, v: &Address, data: Vec<u8>, with_system: bool) -> Instruction {
        let mut accounts = vec![
            AccountMeta::new(trader.pubkey(), true),
            AccountMeta::new(*m, false),
            AccountMeta::new(self.pos(m, &trader.pubkey()), false),
            AccountMeta::new(*v, false),
            AccountMeta::new(*ta, false),
            AccountMeta::new_readonly(TOKEN, false),
        ];
        if with_system {
            accounts.push(AccountMeta::new_readonly(SYSTEM, false));
        }
        Instruction { program_id: self.program, accounts, data }
    }

    fn buy(&mut self, t: &Keypair, ta: &Address, m: &Address, v: &Address, outcome: u8, amount: u64, max_cost: u64) -> Result<u64, String> {
        let ix = self.trade_ix(t, ta, m, v, LmsrInstruction::Buy { outcome, amount, max_cost }.pack(), true);
        self.send(t, ix)
    }
    fn sell(&mut self, t: &Keypair, ta: &Address, m: &Address, v: &Address, outcome: u8, amount: u64, min_payout: u64) -> Result<u64, String> {
        let ix = self.trade_ix(t, ta, m, v, LmsrInstruction::Sell { outcome, amount, min_payout }.pack(), false);
        self.send(t, ix)
    }
    fn redeem(&mut self, t: &Keypair, ta: &Address, m: &Address, v: &Address) -> Result<u64, String> {
        let ix = self.trade_ix(t, ta, m, v, LmsrInstruction::Redeem.pack(), false);
        self.send(t, ix)
    }
    fn resolve(&mut self, auth: &Keypair, m: &Address, winner: u8) -> Result<u64, String> {
        let ix = Instruction {
            program_id: self.program,
            accounts: vec![AccountMeta::new_readonly(auth.pubkey(), true), AccountMeta::new(*m, false)],
            data: LmsrInstruction::Resolve { winner }.pack(),
        };
        self.send(auth, ix)
    }
    fn market(&self, m: &Address) -> Market {
        Market::unpack(&self.svm.get_account(m).unwrap().data).unwrap()
    }
    fn position(&self, m: &Address, owner: &Address) -> Position {
        Position::unpack(&self.svm.get_account(&self.pos(m, owner)).unwrap().data).unwrap()
    }
}

fn custom(code: u32) -> String {
    format!("Custom({code})")
}

#[test]
fn full_lifecycle_create_buy_sell_resolve_redeem() {
    let mut e = Env::new();
    let (auth, auth_ta) = e.user(1_000 * UNIT);
    let b = 100 * WAD;
    let (m, v) = e.create(&auth, &auth_ta, 1, 3, b).unwrap();
    let subsidy = to_units_up(lmsr_max_loss(b, 3).unwrap(), 10u128.pow(12)).unwrap();
    assert_eq!(e.balance(&v), subsidy);
    assert!(subsidy >= 109_861_229); // 100 * ln 3 tokens, in base units
    assert_eq!(e.market(&m).subsidy, subsidy);

    let (alice, alice_ta) = e.user(500 * UNIT);
    let (bob, bob_ta) = e.user(500 * UNIT);
    e.buy(&alice, &alice_ta, &m, &v, 0, 40 * UNIT, u64::MAX).unwrap();
    e.buy(&bob, &bob_ta, &m, &v, 2, 25 * UNIT, u64::MAX).unwrap();
    assert_eq!(e.position(&m, &alice.pubkey()).shares[0], 40 * UNIT);
    e.sell(&alice, &alice_ta, &m, &v, 0, 10 * UNIT, 0).unwrap();
    assert_eq!(e.position(&m, &alice.pubkey()).shares[0], 30 * UNIT);
    let mk = e.market(&m);
    assert_eq!(mk.q[0], 30 * UNIT as u128 * 10u128.pow(12));

    e.resolve(&auth, &m, 0).unwrap();
    let before = e.balance(&alice_ta);
    e.redeem(&alice, &alice_ta, &m, &v).unwrap();
    assert_eq!(e.balance(&alice_ta), before + 30 * UNIT);
    // Bob held the losing outcome: nothing to redeem.
    assert!(e.redeem(&bob, &bob_ta, &m, &v).unwrap_err().contains(&custom(10)));
    // Double redeem is rejected.
    assert!(e.redeem(&alice, &alice_ta, &m, &v).unwrap_err().contains(&custom(10)));
    // Trading after resolution is rejected.
    assert!(e.buy(&bob, &bob_ta, &m, &v, 0, UNIT, u64::MAX).unwrap_err().contains(&custom(5)));
}

#[test]
fn round_trip_on_chain_loses_money() {
    let mut e = Env::new();
    let (auth, auth_ta) = e.user(1_000 * UNIT);
    let (m, v) = e.create(&auth, &auth_ta, 7, 4, 50 * WAD).unwrap();
    let (t, ta) = e.user(1_000 * UNIT);
    for amount in [1u64, 7, 999, UNIT, 13 * UNIT + 5, 120 * UNIT] {
        for outcome in 0..4u8 {
            let start = e.balance(&ta);
            e.buy(&t, &ta, &m, &v, outcome, amount, u64::MAX).unwrap();
            e.sell(&t, &ta, &m, &v, outcome, amount, 0).unwrap();
            assert!(e.balance(&ta) < start, "round trip of {amount} on {outcome} did not lose");
        }
    }
    assert_eq!(e.market(&m).q, [0u128; 8]);
}

#[test]
fn random_sequences_stay_solvent_and_pay_every_winner() {
    // Deterministic LCG so the sequence is reproducible without extra deps.
    let mut seed: u64 = 20261119;
    let mut rnd = move |hi: u64| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) % hi
    };
    for (round, n) in [2u8, 3, 5, 8].into_iter().enumerate() {
        let mut e = Env::new();
        let (auth, auth_ta) = e.user(10_000 * UNIT);
        let b = (5 + rnd(200) as u128) * WAD;
        let (m, v) = e.create(&auth, &auth_ta, round as u64, n, b).unwrap();
        let traders: Vec<(Keypair, Address)> = (0..4).map(|_| e.user(100_000 * UNIT)).collect();
        for _ in 0..40 {
            let (t, ta) = &traders[rnd(4) as usize];
            let o = rnd(n as u64) as u8;
            let held = e.svm.get_account(&e.pos(&m, &t.pubkey())).map(|_| e.position(&m, &t.pubkey()).shares[o as usize]).unwrap_or(0);
            if held > 0 && rnd(2) == 0 {
                let amt = 1 + rnd(held);
                e.sell(t, ta, &m, &v, o, amt, 0).unwrap();
            } else {
                let amt = 1 + rnd(3 * (b / WAD) as u64 * UNIT);
                e.buy(t, ta, &m, &v, o, amt, u64::MAX).unwrap();
            }
            let mk = e.market(&m);
            let worst = mk.q.iter().max().unwrap() / 10u128.pow(12);
            assert!(e.balance(&v) as u128 >= worst, "vault below largest outstanding outcome");
        }
        let winner = rnd(n as u64) as u8;
        e.resolve(&auth, &m, winner).unwrap();
        let owed: u64 = traders.iter().map(|(t, _)| e.svm.get_account(&e.pos(&m, &t.pubkey())).map(|_| e.position(&m, &t.pubkey()).shares[winner as usize]).unwrap_or(0)).sum();
        let vault_before = e.balance(&v);
        assert!(vault_before >= owed);
        for (t, ta) in &traders {
            let s = e.svm.get_account(&e.pos(&m, &t.pubkey())).map(|_| e.position(&m, &t.pubkey()).shares[winner as usize]).unwrap_or(0);
            if s > 0 {
                e.redeem(t, ta, &m, &v).unwrap();
            }
        }
        // Every winner was paid in full from the vault.
        assert_eq!(e.balance(&v), vault_before - owed);
    }
}

#[test]
fn guards_and_errors() {
    let mut e = Env::new();
    let (auth, auth_ta) = e.user(1_000 * UNIT);
    // Outcome count and liquidity bounds.
    assert!(e.create(&auth, &auth_ta, 1, 1, WAD).unwrap_err().contains(&custom(1)));
    assert!(e.create(&auth, &auth_ta, 1, 9, WAD).unwrap_err().contains(&custom(1)));
    assert!(e.create(&auth, &auth_ta, 1, 2, 0).unwrap_err().contains(&custom(2)));
    let (m, v) = e.create(&auth, &auth_ta, 1, 2, 10 * WAD).unwrap();
    // Same market id twice fails.
    assert!(e.create(&auth, &auth_ta, 1, 2, 10 * WAD).is_err());
    let (t, ta) = e.user(100 * UNIT);
    // Slippage: max_cost too low / min_payout too high.
    assert!(e.buy(&t, &ta, &m, &v, 0, 5 * UNIT, 1).unwrap_err().contains(&custom(4)));
    e.buy(&t, &ta, &m, &v, 0, 5 * UNIT, u64::MAX).unwrap();
    assert!(e.sell(&t, &ta, &m, &v, 0, 5 * UNIT, u64::MAX).unwrap_err().contains(&custom(4)));
    // Bad outcome, zero amount, selling more than held.
    assert!(e.buy(&t, &ta, &m, &v, 2, UNIT, u64::MAX).unwrap_err().contains(&custom(3)));
    assert!(e.buy(&t, &ta, &m, &v, 0, 0, u64::MAX).unwrap_err().contains(&custom(10)));
    assert!(e.sell(&t, &ta, &m, &v, 0, 6 * UNIT, 0).unwrap_err().contains(&custom(9)));
    assert!(e.sell(&t, &ta, &m, &v, 1, 1, 0).unwrap_err().contains(&custom(9)));
    // Redeem before resolution; resolve by a stranger.
    assert!(e.redeem(&t, &ta, &m, &v).unwrap_err().contains(&custom(6)));
    assert!(e.resolve(&t, &m, 0).unwrap_err().contains(&custom(7)));
    assert!(e.resolve(&auth, &m, 2).unwrap_err().contains(&custom(3)));
    e.resolve(&auth, &m, 1).unwrap();
    assert!(e.resolve(&auth, &m, 0).unwrap_err().contains(&custom(5)));
}

#[test]
fn compute_units_n8() {
    let mut e = Env::new();
    let (auth, auth_ta) = e.user(10_000 * UNIT);
    let (m, v) = e.create(&auth, &auth_ta, 3, 8, 100 * WAD).unwrap();
    let (t, ta) = e.user(10_000 * UNIT);
    let buy = e.buy(&t, &ta, &m, &v, 3, 50 * UNIT, u64::MAX).unwrap();
    let sell = e.sell(&t, &ta, &m, &v, 3, 20 * UNIT, 0).unwrap();
    println!("compute units, n=8: buy(first, creates position) = {buy}, sell = {sell}");
    assert!(buy < 600_000 && sell < 600_000, "client requests 600k CU");
}
