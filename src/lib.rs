use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod slot_hash;
pub mod swap;
pub mod token_utils;
pub mod vrf;

use instructions::*;
use state::*;

declare_id!("sc6TuL2w5UWBM9ygRZbH1MVjc7oLHZTgv1mg3Q1c21E");

#[program]
pub mod coinflip {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, vrf_program: Pubkey) -> Result<()> {
        instructions::initialize::handler(ctx, vrf_program)
    }

    pub fn set_authority(ctx: Context<SetAuthority>, authority: Pubkey) -> Result<()> {
        instructions::admin::set_authority(ctx, authority)
    }

    pub fn set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
        instructions::admin::set_paused(ctx, paused)
    }

    pub fn set_fee_bps(ctx: Context<SetFeeBps>, fee_bps: u16) -> Result<()> {
        instructions::admin::set_fee_bps(ctx, fee_bps)
    }

    pub fn enable_token(
        ctx: Context<EnableToken>,
        mint: Pubkey,
        min_amount: u64,
        is_enabled: bool,
        pool: Pubkey,
        quote_mint: Pubkey,
        cross_disabled: bool,
    ) -> Result<()> {
        instructions::admin::enable_token(
            ctx,
            mint,
            min_amount,
            is_enabled,
            pool,
            quote_mint,
            cross_disabled,
        )
    }

    pub fn set_sol_usdc_pool(
        ctx: Context<SetSolUsdcPool>,
        usdc_mint: Pubkey,
        pool: Pubkey,
    ) -> Result<()> {
        instructions::admin::set_sol_usdc_pool(ctx, usdc_mint, pool)
    }

    pub fn set_sol_min_amount(ctx: Context<SetSolMinAmount>, sol_min_amount: u64) -> Result<()> {
        instructions::admin::set_sol_min_amount(ctx, sol_min_amount)
    }

    pub fn migrate_config(ctx: Context<MigrateConfig>) -> Result<()> {
        instructions::admin::migrate_config(ctx)
    }

    pub fn migrate_token_config(ctx: Context<MigrateTokenConfig>, mint: Pubkey) -> Result<()> {
        instructions::admin::migrate_token_config(ctx, mint)
    }

    pub fn create(
        ctx: Context<Create>,
        amount: u64,
        side: Side,
        creator_entropy: [u8; 32],
        nonce: u64,
        mint: Pubkey,
    ) -> Result<()> {
        instructions::create::handler(ctx, amount, side, creator_entropy, nonce, mint)
    }

    pub fn join(
        ctx: Context<Join>,
        joiner_side: Side,
        joiner_entropy: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        instructions::join::handler(ctx, joiner_side, joiner_entropy, amount)
    }

    pub fn participate<'info>(
        ctx: Context<'_, '_, 'info, 'info, Participate<'info>>,
        joiner_side: Side,
        joiner_entropy: [u8; 32],
        amount: u64,
        pay_mint: Pubkey,
        max_pay: u64,
        quote_out: u64,
        hop1_len: u8,
    ) -> Result<()> {
        instructions::participate::handler(
            ctx,
            joiner_side,
            joiner_entropy,
            amount,
            pay_mint,
            max_pay,
            quote_out,
            hop1_len,
        )
    }

    pub fn resolve(ctx: Context<Resolve>) -> Result<()> {
        instructions::resolve::handler(ctx)
    }

    pub fn refund_expired(ctx: Context<RefundExpired>) -> Result<()> {
        instructions::refund::handler(ctx)
    }

    pub fn initiate_cancel(ctx: Context<InitiateCancel>) -> Result<()> {
        instructions::cancel::initiate(ctx)
    }

    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        instructions::cancel::handler(ctx)
    }
}
