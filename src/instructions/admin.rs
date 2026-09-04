use crate::errors::CoinflipError;
use crate::state::{
    Config, TokenConfig, CONFIG_SEED, DEFAULT_SOL_MAX_AMOUNT, DEFAULT_SOL_MIN_AMOUNT, MAX_FEE_BPS,
    TOKEN_SEED,
};
use crate::vrf::ORAO_VRF_ID;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetPaused<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct SetAuthority<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn set_authority(ctx: Context<SetAuthority>, authority: Pubkey) -> Result<()> {
    require!(authority != Pubkey::default(), CoinflipError::Unauthorized);
    ctx.accounts.config.authority = authority;
    Ok(())
}

pub fn set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    Ok(())
}

#[derive(Accounts)]
pub struct SetFeeBps<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn set_fee_bps(ctx: Context<SetFeeBps>, fee_bps: u16) -> Result<()> {
    require!(fee_bps <= MAX_FEE_BPS, CoinflipError::InvalidFeeBps);
    ctx.accounts.config.fee_bps = fee_bps;
    Ok(())
}

#[derive(Accounts)]
#[instruction(mint: Pubkey)]
pub struct EnableToken<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + TokenConfig::INIT_SPACE,
        seeds = [TOKEN_SEED, mint.as_ref()],
        bump
    )]
    pub token_config: Account<'info, TokenConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn enable_token(
    ctx: Context<EnableToken>,
    mint: Pubkey,
    min_amount: u64,
    max_amount: u64,
    is_enabled: bool,
    pool: Pubkey,
    quote_mint: Pubkey,
    cross_disabled: bool,
) -> Result<()> {
    if is_enabled {
        require!(min_amount > 0, CoinflipError::InvalidMinAmount);
        require!(max_amount >= min_amount, CoinflipError::InvalidMaxAmount);
        require!(pool != Pubkey::default(), CoinflipError::PoolRequired);
        let quote_ok = crate::state::is_sol_mint(&quote_mint)
            || (ctx.accounts.config.usdc_mint != Pubkey::default()
                && quote_mint == ctx.accounts.config.usdc_mint);
        require!(quote_ok, CoinflipError::InvalidQuoteMint);
    }

    let token_config = &mut ctx.accounts.token_config;
    token_config.mint = mint;
    token_config.min_amount = min_amount;
    token_config.is_enabled = is_enabled;
    token_config.bump = ctx.bumps.token_config;
    token_config.pool = pool;
    token_config.quote_mint = quote_mint;
    token_config.cross_disabled = cross_disabled;
    token_config.max_amount = max_amount;
    Ok(())
}

#[derive(Accounts)]
pub struct SetSolUsdcPool<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn set_sol_usdc_pool(
    ctx: Context<SetSolUsdcPool>,
    usdc_mint: Pubkey,
    pool: Pubkey,
) -> Result<()> {
    require!(usdc_mint != Pubkey::default(), CoinflipError::InvalidMint);
    require!(pool != Pubkey::default(), CoinflipError::PoolRequired);
    ctx.accounts.config.usdc_mint = usdc_mint;
    ctx.accounts.config.sol_usdc_pool = pool;
    Ok(())
}

#[derive(Accounts)]
pub struct SetSolMinAmount<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn set_sol_min_amount(ctx: Context<SetSolMinAmount>, sol_min_amount: u64) -> Result<()> {
    require!(sol_min_amount > 0, CoinflipError::InvalidMinAmount);
    let max = ctx.accounts.config.sol_max_amount;
    if max > 0 {
        require!(sol_min_amount <= max, CoinflipError::InvalidMaxAmount);
    }
    ctx.accounts.config.sol_min_amount = sol_min_amount;
    Ok(())
}

#[derive(Accounts)]
pub struct SetSolMaxAmount<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ CoinflipError::Unauthorized
    )]
    pub config: Account<'info, Config>,
    pub authority: Signer<'info>,
}

pub fn set_sol_max_amount(ctx: Context<SetSolMaxAmount>, sol_max_amount: u64) -> Result<()> {
    require!(sol_max_amount > 0, CoinflipError::InvalidMaxAmount);
    require!(
        sol_max_amount >= ctx.accounts.config.sol_min_amount,
        CoinflipError::InvalidMaxAmount
    );
    ctx.accounts.config.sol_max_amount = sol_max_amount;
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateConfig<'info> {
    /// CHECK: config PDA; resized from the previous layout
    #[account(mut, seeds = [CONFIG_SEED], bump)]
    pub config: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn migrate_config(ctx: Context<MigrateConfig>) -> Result<()> {
    resize_owned_account(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        8 + Config::INIT_SPACE,
    )?;
    require_config_authority(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority.key(),
    )?;
    init_new_config_fields(&ctx.accounts.config.to_account_info())
}

#[derive(Accounts)]
#[instruction(mint: Pubkey)]
pub struct MigrateTokenConfig<'info> {
    /// CHECK: authority is read from raw config bytes so this works before migrate_config
    #[account(seeds = [CONFIG_SEED], bump)]
    pub config: UncheckedAccount<'info>,
    /// CHECK: token PDA; resized from the previous layout
    #[account(mut, seeds = [TOKEN_SEED, mint.as_ref()], bump)]
    pub token_config: UncheckedAccount<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn migrate_token_config(ctx: Context<MigrateTokenConfig>, _mint: Pubkey) -> Result<()> {
    require_config_authority(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority.key(),
    )?;
    resize_owned_account(
        &ctx.accounts.token_config.to_account_info(),
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        8 + TokenConfig::INIT_SPACE,
    )
}

fn require_config_authority(config: &AccountInfo, authority: &Pubkey) -> Result<()> {
    let data = config.try_borrow_data()?;
    require!(data.len() >= 40, CoinflipError::Unauthorized);
    let stored = Pubkey::try_from(&data[8..40]).unwrap();
    require_keys_eq!(stored, *authority, CoinflipError::Unauthorized);
    Ok(())
}

fn resize_owned_account<'info>(
    account: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    new_len: usize,
) -> Result<()> {
    require_keys_eq!(*account.owner, crate::ID, CoinflipError::Unauthorized);
    if account.data_len() >= new_len {
        return Ok(());
    }
    let need = Rent::get()?.minimum_balance(new_len);
    let extra = need.saturating_sub(account.lamports());
    if extra > 0 {
        anchor_lang::system_program::transfer(
            CpiContext::new(
                system_program.clone(),
                anchor_lang::system_program::Transfer {
                    from: payer.clone(),
                    to: account.clone(),
                },
            ),
            extra,
        )?;
    }
    account.realloc(new_len, true)?;
    Ok(())
}

fn init_new_config_fields(config: &AccountInfo) -> Result<()> {
    let mut data = config.try_borrow_mut_data()?;
    let sol_max_off = 8 + Config::INIT_SPACE - 8;
    let vrf_off = sol_max_off - 32;
    let sol_off = vrf_off - 8;
    require!(data.len() >= sol_max_off + 8, CoinflipError::Unauthorized);
    if data[sol_off..sol_off + 8] == [0u8; 8] {
        data[sol_off..sol_off + 8].copy_from_slice(&DEFAULT_SOL_MIN_AMOUNT.to_le_bytes());
    }
    if data[vrf_off..vrf_off + 32] == [0u8; 32] {
        data[vrf_off..vrf_off + 32].copy_from_slice(ORAO_VRF_ID.as_ref());
    }
    if data[sol_max_off..sol_max_off + 8] == [0u8; 8] {
        data[sol_max_off..sol_max_off + 8].copy_from_slice(&DEFAULT_SOL_MAX_AMOUNT.to_le_bytes());
    }
    Ok(())
}
