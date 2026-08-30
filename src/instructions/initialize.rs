use crate::state::{Config, CONFIG_SEED, DEFAULT_FEE_BPS, DEFAULT_SOL_MIN_AMOUNT};
use crate::vrf::ORAO_VRF_ID;
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

pub fn handler(ctx: Context<Initialize>, resolver: Pubkey) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.authority = ctx.accounts.authority.key();
    config.resolver = resolver;
    config.fee_bps = DEFAULT_FEE_BPS;
    config.paused = false;
    config.bump = ctx.bumps.config;
    config.usdc_mint = Pubkey::default();
    config.sol_usdc_pool = Pubkey::default();
    config.sol_min_amount = DEFAULT_SOL_MIN_AMOUNT;
    config.vrf_program = ORAO_VRF_ID;
    Ok(())
}
