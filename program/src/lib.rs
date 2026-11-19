//! lmsr-sol: an N-outcome LMSR prediction-market maker as a native Solana
//! program. Collateral is any SPL token (devnet uses a test mint). Shares are
//! denominated in the collateral's base units: one winning share redeems for
//! one base unit.
//!
//! Instructions (first byte is the tag, little-endian fields follow):
//! * 0 `CreateMarket { market_id: u64, n: u8, b_wad: u128 }`
//! * 1 `Buy  { outcome: u8, amount: u64, max_cost: u64 }`
//! * 2 `Sell { outcome: u8, amount: u64, min_payout: u64 }`
//! * 3 `Resolve { winner: u8 }`
//! * 4 `Redeem`
//!
//! Rounding: costs are computed in WAD by `lmsr-math` (buy rounds up, sell
//! rounds down) and then converted to base units with the same policy
//! (buy cost ceil, sell payout floor).
#![allow(unexpected_cfgs)]

pub mod state;

use lmsr_math::{
    lmsr_buy_cost_n, lmsr_max_loss, lmsr_sell_payout_n, to_units_down, to_units_up, MathError,
    MAX_OUTCOMES, WAD,
};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use state::{Market, Position, MARKET_LEN, POSITION_LEN, STATUS_OPEN, STATUS_RESOLVED};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

/// SPL Token program (classic).
pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
/// System program.
pub const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");
/// SPL token account size.
pub const TOKEN_ACCOUNT_LEN: u64 = 165;

pub const MARKET_SEED: &[u8] = b"market";
pub const VAULT_SEED: &[u8] = b"vault";
pub const POSITION_SEED: &[u8] = b"position";

/// Liquidity bounds: 0.000001 .. 1e12 tokens (WAD).
pub const MIN_B: u128 = WAD / 1_000_000;
pub const MAX_B: u128 = 1_000_000_000_000 * WAD;

/// Program errors (`ProgramError::Custom(code)`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LmsrError {
    InvalidInstruction = 0,
    BadOutcomeCount = 1,
    BadLiquidity = 2,
    BadOutcome = 3,
    SlippageExceeded = 4,
    MarketNotOpen = 5,
    MarketNotResolved = 6,
    Unauthorized = 7,
    WrongAccount = 8,
    InsufficientShares = 9,
    ZeroAmount = 10,
    MathOverflow = 11,
    BadMint = 12,
}

impl From<LmsrError> for ProgramError {
    fn from(e: LmsrError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

fn math(e: MathError) -> ProgramError {
    match e {
        MathError::BadOutcome => LmsrError::BadOutcome.into(),
        MathError::InsufficientShares => LmsrError::InsufficientShares.into(),
        _ => LmsrError::MathOverflow.into(),
    }
}

/// Decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LmsrInstruction {
    CreateMarket { market_id: u64, n: u8, b_wad: u128 },
    Buy { outcome: u8, amount: u64, max_cost: u64 },
    Sell { outcome: u8, amount: u64, min_payout: u64 },
    Resolve { winner: u8 },
    Redeem,
}

fn rd_u64(d: &[u8], at: usize) -> Result<u64, ProgramError> {
    d.get(at..at + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| LmsrError::InvalidInstruction.into())
}

impl LmsrInstruction {
    pub fn unpack(data: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = data.split_first().ok_or(LmsrError::InvalidInstruction)?;
        let byte = |at: usize| rest.get(at).copied().ok_or(LmsrError::InvalidInstruction);
        Ok(match tag {
            0 => {
                let b = rest.get(9..25).ok_or(LmsrError::InvalidInstruction)?;
                LmsrInstruction::CreateMarket {
                    market_id: rd_u64(rest, 0)?,
                    n: byte(8)?,
                    b_wad: u128::from_le_bytes(b.try_into().unwrap()),
                }
            }
            1 => LmsrInstruction::Buy { outcome: byte(0)?, amount: rd_u64(rest, 1)?, max_cost: rd_u64(rest, 9)? },
            2 => LmsrInstruction::Sell { outcome: byte(0)?, amount: rd_u64(rest, 1)?, min_payout: rd_u64(rest, 9)? },
            3 => LmsrInstruction::Resolve { winner: byte(0)? },
            4 => LmsrInstruction::Redeem,
            _ => return Err(LmsrError::InvalidInstruction.into()),
        })
    }

    pub fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(26);
        match self {
            LmsrInstruction::CreateMarket { market_id, n, b_wad } => {
                v.push(0);
                v.extend_from_slice(&market_id.to_le_bytes());
                v.push(*n);
                v.extend_from_slice(&b_wad.to_le_bytes());
            }
            LmsrInstruction::Buy { outcome, amount, max_cost } => {
                v.push(1);
                v.push(*outcome);
                v.extend_from_slice(&amount.to_le_bytes());
                v.extend_from_slice(&max_cost.to_le_bytes());
            }
            LmsrInstruction::Sell { outcome, amount, min_payout } => {
                v.push(2);
                v.push(*outcome);
                v.extend_from_slice(&amount.to_le_bytes());
                v.extend_from_slice(&min_payout.to_le_bytes());
            }
            LmsrInstruction::Resolve { winner } => {
                v.push(3);
                v.push(*winner);
            }
            LmsrInstruction::Redeem => v.push(4),
        }
        v
    }
}

/// PDA helpers (also used by clients and tests).
pub fn market_pda(program_id: &Pubkey, authority: &Pubkey, market_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MARKET_SEED, authority.as_ref(), &market_id.to_le_bytes()], program_id)
}
pub fn vault_pda(program_id: &Pubkey, market: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, market.as_ref()], program_id)
}
pub fn position_pda(program_id: &Pubkey, market: &Pubkey, owner: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[POSITION_SEED, market.as_ref(), owner.as_ref()], program_id)
}

pub fn process_instruction(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    match LmsrInstruction::unpack(data)? {
        LmsrInstruction::CreateMarket { market_id, n, b_wad } => create_market(program_id, accounts, market_id, n, b_wad),
        LmsrInstruction::Buy { outcome, amount, max_cost } => buy(program_id, accounts, outcome, amount, max_cost),
        LmsrInstruction::Sell { outcome, amount, min_payout } => sell(program_id, accounts, outcome, amount, min_payout),
        LmsrInstruction::Resolve { winner } => resolve(program_id, accounts, winner),
        LmsrInstruction::Redeem => redeem(program_id, accounts),
    }
}

// ---------------------------------------------------------------------------
// Raw CPI builders (System CreateAccount, Token InitializeAccount3 / Transfer)
// so the program has no dependency beyond solana-program.
// ---------------------------------------------------------------------------

fn ix_create_account(from: &Pubkey, to: &Pubkey, lamports: u64, space: u64, owner: &Pubkey) -> Instruction {
    let mut data = Vec::with_capacity(52);
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&lamports.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*to, true)],
        data,
    }
}

fn ix_init_token_account3(account: &Pubkey, mint: &Pubkey, owner: &Pubkey) -> Instruction {
    let mut data = Vec::with_capacity(33);
    data.push(18);
    data.extend_from_slice(owner.as_ref());
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![AccountMeta::new(*account, false), AccountMeta::new_readonly(*mint, false)],
        data,
    }
}

fn ix_token_transfer(src: &Pubkey, dst: &Pubkey, authority: &Pubkey, amount: u64) -> Instruction {
    let mut data = Vec::with_capacity(9);
    data.push(3);
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*src, false),
            AccountMeta::new(*dst, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data,
    }
}

/// Create a PDA account owned by `owner`, tolerating pre-funded lamports.
#[allow(clippy::too_many_arguments)]
fn create_pda<'a>(
    payer: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    system: &AccountInfo<'a>,
    space: u64,
    owner: &Pubkey,
    seeds: &[&[u8]],
) -> ProgramResult {
    if target.lamports() > 0 || !target.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let lamports = Rent::get()?.minimum_balance(space as usize);
    invoke_signed(
        &ix_create_account(payer.key, target.key, lamports, space, owner),
        &[payer.clone(), target.clone(), system.clone()],
        &[seeds],
    )
}

fn check_token_program(a: &AccountInfo) -> ProgramResult {
    if *a.key != TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

fn load_market(program_id: &Pubkey, a: &AccountInfo) -> Result<Market, ProgramError> {
    if a.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    Market::unpack(&a.try_borrow_data()?)
}

fn scale_of(decimals: u8) -> u128 {
    10u128.pow(18 - decimals as u32)
}

// ---------------------------------------------------------------------------

/// Accounts: 0 authority [signer, writable], 1 market PDA [w], 2 vault PDA [w],
/// 3 mint, 4 authority token account [w], 5 token program, 6 system program.
/// Locks the subsidy lmsr_max_loss(b, n) (ceil to base units) in the vault.
fn create_market(program_id: &Pubkey, accounts: &[AccountInfo], market_id: u64, n: u8, b_wad: u128) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let market_ai = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let mint_ai = next_account_info(it)?;
    let auth_token = next_account_info(it)?;
    let token_prog = next_account_info(it)?;
    let system = next_account_info(it)?;

    if !authority.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_prog)?;
    if *system.key != SYSTEM_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    if !(2..=MAX_OUTCOMES as u8).contains(&n) {
        return Err(LmsrError::BadOutcomeCount.into());
    }
    if !(MIN_B..=MAX_B).contains(&b_wad) {
        return Err(LmsrError::BadLiquidity.into());
    }
    // Mint: owned by the token program, 82 bytes, decimals at offset 44, initialized at 45.
    if *mint_ai.owner != TOKEN_PROGRAM_ID {
        return Err(LmsrError::BadMint.into());
    }
    let decimals = {
        let d = mint_ai.try_borrow_data()?;
        if d.len() != 82 || d[45] != 1 || d[44] > 18 {
            return Err(LmsrError::BadMint.into());
        }
        d[44]
    };

    let (mkey, mbump) = market_pda(program_id, authority.key, market_id);
    if mkey != *market_ai.key {
        return Err(LmsrError::WrongAccount.into());
    }
    let (vkey, vbump) = vault_pda(program_id, &mkey);
    if vkey != *vault_ai.key {
        return Err(LmsrError::WrongAccount.into());
    }

    let id_bytes = market_id.to_le_bytes();
    create_pda(authority, market_ai, system, MARKET_LEN as u64, program_id,
        &[MARKET_SEED, authority.key.as_ref(), &id_bytes, &[mbump]])?;
    create_pda(authority, vault_ai, system, TOKEN_ACCOUNT_LEN, &TOKEN_PROGRAM_ID,
        &[VAULT_SEED, mkey.as_ref(), &[vbump]])?;
    invoke(&ix_init_token_account3(&vkey, mint_ai.key, &mkey), &[vault_ai.clone(), mint_ai.clone()])?;

    let scale = scale_of(decimals);
    let subsidy = to_units_up(lmsr_max_loss(b_wad, n as usize).map_err(math)?, scale).map_err(math)?;
    invoke(
        &ix_token_transfer(auth_token.key, &vkey, authority.key, subsidy),
        &[auth_token.clone(), vault_ai.clone(), authority.clone(), token_prog.clone()],
    )?;

    let m = Market {
        bump: mbump,
        vault_bump: vbump,
        n,
        status: STATUS_OPEN,
        winner: 0,
        decimals,
        authority: *authority.key,
        mint: *mint_ai.key,
        vault: vkey,
        market_id,
        b_wad,
        subsidy,
        q: [0; MAX_OUTCOMES],
    };
    m.pack(&mut market_ai.try_borrow_mut_data()?)?;
    msg!("lmsr: market created n={} subsidy={}", n, subsidy);
    Ok(())
}

/// Accounts: 0 trader [signer, writable], 1 market [w], 2 position PDA [w],
/// 3 vault [w], 4 trader token account [w], 5 token program, 6 system program.
fn buy(program_id: &Pubkey, accounts: &[AccountInfo], outcome: u8, amount: u64, max_cost: u64) -> ProgramResult {
    let it = &mut accounts.iter();
    let trader = next_account_info(it)?;
    let market_ai = next_account_info(it)?;
    let pos_ai = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let trader_token = next_account_info(it)?;
    let token_prog = next_account_info(it)?;
    let system = next_account_info(it)?;

    if !trader.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_prog)?;
    let mut m = load_market(program_id, market_ai)?;
    if m.status != STATUS_OPEN {
        return Err(LmsrError::MarketNotOpen.into());
    }
    if outcome >= m.n {
        return Err(LmsrError::BadOutcome.into());
    }
    if amount == 0 {
        return Err(LmsrError::ZeroAmount.into());
    }
    if *vault_ai.key != m.vault {
        return Err(LmsrError::WrongAccount.into());
    }
    let mut pos = open_position(program_id, market_ai.key, trader, pos_ai, Some(system))?;

    let scale = scale_of(m.decimals);
    let n = m.n as usize;
    let delta = amount as u128 * scale;
    let cost_wad = lmsr_buy_cost_n(&m.q[..n], m.b_wad, outcome as usize, delta).map_err(math)?;
    let cost = to_units_up(cost_wad, scale).map_err(math)?;
    if cost > max_cost {
        return Err(LmsrError::SlippageExceeded.into());
    }
    invoke(
        &ix_token_transfer(trader_token.key, vault_ai.key, trader.key, cost),
        &[trader_token.clone(), vault_ai.clone(), trader.clone(), token_prog.clone()],
    )?;
    let i = outcome as usize;
    m.q[i] = m.q[i].checked_add(delta).ok_or(LmsrError::MathOverflow)?;
    pos.shares[i] = pos.shares[i].checked_add(amount).ok_or(LmsrError::MathOverflow)?;
    m.pack(&mut market_ai.try_borrow_mut_data()?)?;
    pos.pack(&mut pos_ai.try_borrow_mut_data()?)?;
    msg!("lmsr: buy outcome={} shares={} cost={}", outcome, amount, cost);
    Ok(())
}

/// Accounts: 0 trader [signer], 1 market [w], 2 position [w], 3 vault [w],
/// 4 trader token account [w], 5 token program.
fn sell(program_id: &Pubkey, accounts: &[AccountInfo], outcome: u8, amount: u64, min_payout: u64) -> ProgramResult {
    let it = &mut accounts.iter();
    let trader = next_account_info(it)?;
    let market_ai = next_account_info(it)?;
    let pos_ai = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let trader_token = next_account_info(it)?;
    let token_prog = next_account_info(it)?;

    if !trader.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_prog)?;
    let mut m = load_market(program_id, market_ai)?;
    if m.status != STATUS_OPEN {
        return Err(LmsrError::MarketNotOpen.into());
    }
    if outcome >= m.n {
        return Err(LmsrError::BadOutcome.into());
    }
    if amount == 0 {
        return Err(LmsrError::ZeroAmount.into());
    }
    if *vault_ai.key != m.vault {
        return Err(LmsrError::WrongAccount.into());
    }
    let mut pos = open_position(program_id, market_ai.key, trader, pos_ai, None)?;
    let i = outcome as usize;
    if pos.shares[i] < amount {
        return Err(LmsrError::InsufficientShares.into());
    }

    let scale = scale_of(m.decimals);
    let n = m.n as usize;
    let delta = amount as u128 * scale;
    let pay_wad = lmsr_sell_payout_n(&m.q[..n], m.b_wad, i, delta).map_err(math)?;
    let payout = to_units_down(pay_wad, scale).map_err(math)?;
    if payout < min_payout {
        return Err(LmsrError::SlippageExceeded.into());
    }
    m.q[i] -= delta;
    pos.shares[i] -= amount;
    m.pack(&mut market_ai.try_borrow_mut_data()?)?;
    pos.pack(&mut pos_ai.try_borrow_mut_data()?)?;
    if payout > 0 {
        vault_transfer(&m, market_ai, vault_ai, trader_token, token_prog, payout)?;
    }
    msg!("lmsr: sell outcome={} shares={} payout={}", outcome, amount, payout);
    Ok(())
}

/// Accounts: 0 authority [signer], 1 market [w].
fn resolve(program_id: &Pubkey, accounts: &[AccountInfo], winner: u8) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let market_ai = next_account_info(it)?;
    let mut m = load_market(program_id, market_ai)?;
    if !authority.is_signer || *authority.key != m.authority {
        return Err(LmsrError::Unauthorized.into());
    }
    if m.status != STATUS_OPEN {
        return Err(LmsrError::MarketNotOpen.into());
    }
    if winner >= m.n {
        return Err(LmsrError::BadOutcome.into());
    }
    m.status = STATUS_RESOLVED;
    m.winner = winner;
    m.pack(&mut market_ai.try_borrow_mut_data()?)?;
    msg!("lmsr: resolved winner={}", winner);
    Ok(())
}

/// Accounts: 0 trader [signer], 1 market [w], 2 position [w], 3 vault [w],
/// 4 trader token account [w], 5 token program. Pays 1 base unit per winning share.
fn redeem(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let trader = next_account_info(it)?;
    let market_ai = next_account_info(it)?;
    let pos_ai = next_account_info(it)?;
    let vault_ai = next_account_info(it)?;
    let trader_token = next_account_info(it)?;
    let token_prog = next_account_info(it)?;
    if !trader.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_prog)?;
    let m = load_market(program_id, market_ai)?;
    if m.status != STATUS_RESOLVED {
        return Err(LmsrError::MarketNotResolved.into());
    }
    if *vault_ai.key != m.vault {
        return Err(LmsrError::WrongAccount.into());
    }
    let mut pos = open_position(program_id, market_ai.key, trader, pos_ai, None)?;
    let w = m.winner as usize;
    let payout = pos.shares[w];
    if payout == 0 {
        return Err(LmsrError::ZeroAmount.into());
    }
    pos.shares = [0; MAX_OUTCOMES];
    pos.pack(&mut pos_ai.try_borrow_mut_data()?)?;
    vault_transfer(&m, market_ai, vault_ai, trader_token, token_prog, payout)?;
    msg!("lmsr: redeemed {}", payout);
    Ok(())
}

fn vault_transfer<'a>(
    m: &Market,
    market_ai: &AccountInfo<'a>,
    vault_ai: &AccountInfo<'a>,
    dst: &AccountInfo<'a>,
    token_prog: &AccountInfo<'a>,
    amount: u64,
) -> ProgramResult {
    let id_bytes = m.market_id.to_le_bytes();
    invoke_signed(
        &ix_token_transfer(vault_ai.key, dst.key, market_ai.key, amount),
        &[vault_ai.clone(), dst.clone(), market_ai.clone(), token_prog.clone()],
        &[&[MARKET_SEED, m.authority.as_ref(), &id_bytes, &[m.bump]]],
    )
}

/// Load the trader's position PDA, creating it on first buy.
fn open_position<'a>(
    program_id: &Pubkey,
    market: &Pubkey,
    trader: &AccountInfo<'a>,
    pos_ai: &AccountInfo<'a>,
    system: Option<&AccountInfo<'a>>,
) -> Result<Position, ProgramError> {
    let (pkey, pbump) = position_pda(program_id, market, trader.key);
    if pkey != *pos_ai.key {
        return Err(LmsrError::WrongAccount.into());
    }
    if pos_ai.data_is_empty() {
        let system = system.ok_or(LmsrError::InsufficientShares)?;
        if *system.key != SYSTEM_PROGRAM_ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        create_pda(trader, pos_ai, system, POSITION_LEN as u64, program_id,
            &[POSITION_SEED, market.as_ref(), trader.key.as_ref(), &[pbump]])?;
        return Ok(Position { bump: pbump, market: *market, owner: *trader.key, shares: [0; MAX_OUTCOMES] });
    }
    if pos_ai.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    let p = Position::unpack(&pos_ai.try_borrow_data()?)?;
    if p.market != *market || p.owner != *trader.key {
        return Err(LmsrError::WrongAccount.into());
    }
    Ok(p)
}
