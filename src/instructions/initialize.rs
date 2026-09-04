use crate::errors::CoinflipError;
use crate::state::{
    Config, CONFIG_SEED, DEFAULT_FEE_BPS, DEFAULT_SOL_MAX_AMOUNT, DEFAULT_SOL_MIN_AMOUNT,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Initialize>, vrf_program: Pubkey) -> Result<()> {
    require!(vrf_program != Pubkey::default(), CoinflipError::InvalidVrfProgram);
    let config = &mut ctx.accounts.config;
    config.authority = ctx.accounts.authority.key();
    config.resolver = Pubkey::default();
    config.fee_bps = DEFAULT_FEE_BPS;
    config.paused = false;
    config.bump = ctx.bumps.config;
    config.usdc_mint = Pubkey::default();
    config.sol_usdc_pool = Pubkey::default();
    config.sol_min_amount = DEFAULT_SOL_MIN_AMOUNT;
    config.vrf_program = vrf_program;
    config.sol_max_amount = DEFAULT_SOL_MAX_AMOUNT;
    Ok(())
}
