use anchor_lang::prelude::*;

pub const CONFIG_SEED: &[u8] = b"config";
pub const GAME_SEED: &[u8] = b"game";
pub const TOKEN_SEED: &[u8] = b"token";
pub const RESULT_PREFIX: &[u8] = b"coinflip_p2p_v1";
pub const BPS_DENOMINATOR: u16 = 10_000;
pub const DEFAULT_FEE_BPS: u16 = 350;
/// 5% of pot. Snapshotted onto each game at create; changing this does not affect open games.
pub const MAX_FEE_BPS: u16 = 500;
/// 0.01 SOL. Admin can raise or lower this for new games via `set_sol_min_amount`.
pub const DEFAULT_SOL_MIN_AMOUNT: u64 = 10_000_000;
/// ~1 SOL advertised (~$100), including the 3.5% fee pad so a 1 SOL flip can be created.
pub const DEFAULT_SOL_MAX_AMOUNT: u64 = 1_040_000_000;
/// Authority must wait this many slots after `initiate_cancel` before `cancel`.
/// ~10s at 300ms slots. Join can still land during the delay.
pub const AUTHORITY_CANCEL_DELAY_SLOTS: u64 = 32;

pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const RAYDIUM_CLMM: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");
#[cfg(feature = "devnet")]
pub const RAYDIUM_CLMM_DEVNET: Pubkey = pubkey!("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
#[cfg(feature = "devnet")]
pub const RAYDIUM_CLMM_DEVNET_LEGACY: Pubkey =
    pubkey!("devi51mZmdwUJGU9hjN27vEz64Gps7uUefqxg27EAtH");
pub const TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

#[account]
#[derive(InitSpace)]
pub struct Config {
    /// House: upgrade authority and fee recipient.
    pub authority: Pubkey,
    /// Unused. Kept so existing config accounts keep their layout.
    pub resolver: Pubkey,
    pub fee_bps: u16,
    pub paused: bool,
    pub bump: u8,
    pub usdc_mint: Pubkey,
    pub sol_usdc_pool: Pubkey,
    pub sol_min_amount: u64,
    pub vrf_program: Pubkey,
    pub sol_max_amount: u64,
}

#[account]
#[derive(InitSpace)]
pub struct TokenConfig {
    pub mint: Pubkey,
    pub min_amount: u64,
    pub is_enabled: bool,
    pub bump: u8,
    pub pool: Pubkey,
    pub quote_mint: Pubkey,
    pub cross_disabled: bool,
    pub max_amount: u64,
}

#[account]
#[derive(InitSpace)]
pub struct Game {
    pub creator: Pubkey,
    pub joiner: Pubkey,
    pub amount: u64,
    pub mint: Pubkey,
    pub token_decimals: u8,
    pub fee_bps: u16,
    pub creator_side: Side,
    pub joiner_side: Side,
    pub creator_entropy: [u8; 32],
    pub joiner_entropy: [u8; 32],
    pub status: GameStatus,
    pub nonce: u64,
    pub bump: u8,
    pub join_slot: u64,
    /// Earliest slot the authority may cancel. 0 = not initiated.
    pub cancel_after_slot: u64,
    /// When true, only the house (`config.authority`) may join.
    pub house_only: bool,
}

impl Config {
    pub fn house(&self) -> Pubkey {
        self.authority
    }
}

impl Game {
    pub fn is_native_sol(&self) -> bool {
        is_native_sol(&self.mint)
    }
}

pub fn is_native_sol(mint: &Pubkey) -> bool {
    *mint == Pubkey::default()
}

pub fn is_sol_mint(mint: &Pubkey) -> bool {
    is_native_sol(mint) || *mint == WSOL_MINT
}

pub fn effective_mint(mint: &Pubkey) -> Pubkey {
    if is_native_sol(mint) {
        WSOL_MINT
    } else {
        *mint
    }
}

pub fn pot_fee(amount: u64, fee_bps: u16) -> Result<u64> {
    let pot = amount
        .checked_mul(2)
        .ok_or(crate::errors::CoinflipError::ArithmeticOverflow)?;
    let fee = u128::from(pot)
        .checked_mul(u128::from(fee_bps))
        .ok_or(crate::errors::CoinflipError::ArithmeticOverflow)?
        / u128::from(BPS_DENOMINATOR);
    u64::try_from(fee).map_err(|_| error!(crate::errors::CoinflipError::ArithmeticOverflow))
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum Side {
    Heads,
    Tails,
    Open,
}

impl Side {
    pub fn opposite(self) -> Result<Self> {
        match self {
            Side::Heads => Ok(Side::Tails),
            Side::Tails => Ok(Side::Heads),
            Side::Open => err!(crate::errors::CoinflipError::InvalidSide),
        }
    }

    pub fn from_result_bit(bit: u8) -> Self {
        if bit == 0 {
            Side::Tails
        } else {
            Side::Heads
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum GameStatus {
    Open,
    Ready,
}

pub fn require_nonzero_entropy(entropy: &[u8; 32]) -> Result<()> {
    require!(*entropy != [0u8; 32], crate::errors::CoinflipError::EntropyAllZero);
    Ok(())
}
