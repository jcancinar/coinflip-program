use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;

declare_id!("8DeY7XokDSxSC5nJjwuoYW8Ajjo36UH9XXpu247tcJWf");

pub const RANDOMNESS_ACCOUNT_SEED: &[u8] = b"orao-vrf-randomness-request";
pub const CONFIG_ACCOUNT_SEED: &[u8] = b"orao-vrf-network-configuration";

#[account]
pub struct NetworkState {
    pub bump: u8,
}

#[account]
pub struct RandomnessV2 {
    pub tag: u8,
    pub client: Pubkey,
    pub seed: [u8; 32],
    pub randomness: [u8; 64],
}

#[program]
pub mod mock_vrf {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.network_state.bump = ctx.bumps.network_state;
        Ok(())
    }

    pub fn request_v2(ctx: Context<RequestV2>, seed: [u8; 32]) -> Result<()> {
        let hashed = hashv(&[b"coinflip_mock_vrf", &seed]).to_bytes();
        let mut randomness = [0u8; 64];
        randomness[..32].copy_from_slice(&hashed);
        randomness[32..].copy_from_slice(&hashed);
        let request = &mut ctx.accounts.request;
        request.tag = 1;
        request.client = ctx.accounts.payer.key();
        request.seed = seed;
        request.randomness = randomness;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + 1,
        seeds = [CONFIG_ACCOUNT_SEED],
        bump
    )]
    pub network_state: Account<'info, NetworkState>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(seed: [u8; 32])]
pub struct RequestV2<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut, seeds = [CONFIG_ACCOUNT_SEED], bump)]
    pub network_state: Account<'info, NetworkState>,
    /// CHECK: unused treasury stand-in
    #[account(mut)]
    pub treasury: AccountInfo<'info>,
    #[account(
        init,
        payer = payer,
        space = 8 + 1 + 32 + 32 + 64,
        seeds = [RANDOMNESS_ACCOUNT_SEED, &seed],
        bump
    )]
    pub request: Account<'info, RandomnessV2>,
    pub system_program: Program<'info, System>,
}
