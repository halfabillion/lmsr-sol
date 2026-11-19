//! Fixed-layout account state (little-endian, no external serializer).
use lmsr_math::MAX_OUTCOMES;
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

pub const MARKET_DISC: u8 = 1;
pub const POSITION_DISC: u8 = 2;
pub const STATUS_OPEN: u8 = 0;
pub const STATUS_RESOLVED: u8 = 1;

/// 8 header + 3*32 keys + 8 id + 16 b + 8 subsidy + 8*16 q = 264
pub const MARKET_LEN: usize = 264;
/// 8 header + 2*32 keys + 8*8 shares = 136
pub const POSITION_LEN: usize = 136;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Market {
    pub bump: u8,
    pub vault_bump: u8,
    pub n: u8,
    pub status: u8,
    pub winner: u8,
    pub decimals: u8,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub market_id: u64,
    /// Liquidity parameter b in WAD (1e18 = one whole token).
    pub b_wad: u128,
    /// Subsidy locked at creation, in base units.
    pub subsidy: u64,
    /// Outstanding shares per outcome, in WAD.
    pub q: [u128; MAX_OUTCOMES],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub bump: u8,
    pub market: Pubkey,
    pub owner: Pubkey,
    /// Shares per outcome, in collateral base units.
    pub shares: [u64; MAX_OUTCOMES],
}

fn key(d: &[u8], at: usize) -> Pubkey {
    Pubkey::new_from_array(d[at..at + 32].try_into().unwrap())
}

impl Market {
    pub fn unpack(d: &[u8]) -> Result<Self, ProgramError> {
        if d.len() < MARKET_LEN || d[0] != MARKET_DISC {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut q = [0u128; MAX_OUTCOMES];
        for (k, qk) in q.iter_mut().enumerate() {
            let at = 136 + 16 * k;
            *qk = u128::from_le_bytes(d[at..at + 16].try_into().unwrap());
        }
        Ok(Market {
            bump: d[1],
            vault_bump: d[2],
            n: d[3],
            status: d[4],
            winner: d[5],
            decimals: d[6],
            authority: key(d, 8),
            mint: key(d, 40),
            vault: key(d, 72),
            market_id: u64::from_le_bytes(d[104..112].try_into().unwrap()),
            b_wad: u128::from_le_bytes(d[112..128].try_into().unwrap()),
            subsidy: u64::from_le_bytes(d[128..136].try_into().unwrap()),
            q,
        })
    }

    pub fn pack(&self, d: &mut [u8]) -> Result<(), ProgramError> {
        if d.len() < MARKET_LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        d[0] = MARKET_DISC;
        d[1] = self.bump;
        d[2] = self.vault_bump;
        d[3] = self.n;
        d[4] = self.status;
        d[5] = self.winner;
        d[6] = self.decimals;
        d[7] = 0;
        d[8..40].copy_from_slice(self.authority.as_ref());
        d[40..72].copy_from_slice(self.mint.as_ref());
        d[72..104].copy_from_slice(self.vault.as_ref());
        d[104..112].copy_from_slice(&self.market_id.to_le_bytes());
        d[112..128].copy_from_slice(&self.b_wad.to_le_bytes());
        d[128..136].copy_from_slice(&self.subsidy.to_le_bytes());
        for (k, qk) in self.q.iter().enumerate() {
            let at = 136 + 16 * k;
            d[at..at + 16].copy_from_slice(&qk.to_le_bytes());
        }
        Ok(())
    }
}

impl Position {
    pub fn unpack(d: &[u8]) -> Result<Self, ProgramError> {
        if d.len() < POSITION_LEN || d[0] != POSITION_DISC {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut shares = [0u64; MAX_OUTCOMES];
        for (k, s) in shares.iter_mut().enumerate() {
            let at = 72 + 8 * k;
            *s = u64::from_le_bytes(d[at..at + 8].try_into().unwrap());
        }
        Ok(Position { bump: d[1], market: key(d, 8), owner: key(d, 40), shares })
    }

    pub fn pack(&self, d: &mut [u8]) -> Result<(), ProgramError> {
        if d.len() < POSITION_LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        d[..8].copy_from_slice(&[POSITION_DISC, self.bump, 0, 0, 0, 0, 0, 0]);
        d[8..40].copy_from_slice(self.market.as_ref());
        d[40..72].copy_from_slice(self.owner.as_ref());
        for (k, s) in self.shares.iter().enumerate() {
            let at = 72 + 8 * k;
            d[at..at + 8].copy_from_slice(&s.to_le_bytes());
        }
        Ok(())
    }
}
