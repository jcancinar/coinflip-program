use crate::errors::CoinflipError;
use crate::state::{Config, Game, GameStatus, CONFIG_SEED, GAME_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CommitResolve<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.creator.as_ref(), &game.nonce.to_le_bytes()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    pub resolver: Signer<'info>,
}

pub fn handler(ctx: Context<CommitResolve>, commit: [u8; 32]) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.resolver.key(),
        ctx.accounts.config.resolver,
        CoinflipError::Unauthorized
    );
    require!(
        ctx.accounts.game.status == GameStatus::Open,
        CoinflipError::AlreadyJoined
    );
    require!(!ctx.accounts.game.commit_is_set(), CoinflipError::CommitAlreadySet);
    require!(commit != [0u8; 32], CoinflipError::CommitMissing);

    ctx.accounts.game.commit = commit;
    Ok(())
}
