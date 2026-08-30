use crate::errors::CoinflipError;
use crate::state::{
    require_nonzero_entropy, Config, Game, GameStatus, Side, CONFIG_SEED, GAME_SEED,
};
use crate::vrf::{request_randomness, vrf_seed};
use crate::token_utils::{require_token_program, require_vault_ata, transfer_tokens};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Join<'info> {
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
    pub mint_account: Option<Box<InterfaceAccount<'info, Mint>>>,
    #[account(
        mut,
        constraint = joiner_token.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = joiner_token.owner == joiner.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub joiner_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(
        mut,
        constraint = vault.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = vault.owner == game.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub vault: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    #[account(
        mut,
        constraint = joiner.key() != game.creator @ CoinflipError::CannotJoinOwnGame
    )]
    pub joiner: Signer<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: ORAO (or configured) VRF program
    pub vrf_program: UncheckedAccount<'info>,
    /// CHECK: VRF network state PDA
    #[account(mut)]
    pub vrf_network: UncheckedAccount<'info>,
    /// CHECK: VRF treasury
    #[account(mut)]
    pub vrf_treasury: UncheckedAccount<'info>,
    /// CHECK: VRF request PDA for this game
    #[account(mut)]
    pub vrf_request: UncheckedAccount<'info>,
}

pub fn handler(
    ctx: Context<Join>,
    joiner_side: Side,
    joiner_entropy: [u8; 32],
    amount: u64,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, CoinflipError::Paused);
    let (mint, decimals) = apply_join(
        &mut ctx.accounts.game,
        ctx.accounts.joiner.key(),
        joiner_side,
        joiner_entropy,
        amount,
    )?;

    if ctx.accounts.game.is_native_sol() {
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.joiner.to_account_info(),
                    to: ctx.accounts.game.to_account_info(),
                },
            ),
            amount,
        )?;
        return request_join_vrf(&ctx);
    }

    let mint_account = ctx
        .accounts
        .mint_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(mint_account.key(), mint, CoinflipError::InvalidMint);
    let joiner_token = ctx
        .accounts
        .joiner_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let vault = ctx
        .accounts
        .vault
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let token_program = require_token_program(&ctx.accounts.token_program)?;
    require_vault_ata(
        &vault.key(),
        &ctx.accounts.game.key(),
        &mint,
        &token_program.key(),
    )?;

    transfer_tokens(
        token_program,
        joiner_token.to_account_info(),
        mint_account.to_account_info(),
        vault.to_account_info(),
        ctx.accounts.joiner.to_account_info(),
        amount,
        decimals,
    )?;

    request_join_vrf(&ctx)?;
    Ok(())
}

fn request_join_vrf(ctx: &Context<Join>) -> Result<()> {
    let game_key = ctx.accounts.game.key();
    let game = &ctx.accounts.game;
    request_randomness(
        &ctx.accounts.vrf_program.to_account_info(),
        &ctx.accounts.joiner.to_account_info(),
        &ctx.accounts.vrf_network.to_account_info(),
        &ctx.accounts.vrf_treasury.to_account_info(),
        &ctx.accounts.vrf_request.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.config.vrf_program,
        vrf_seed(&game_key, &game.joiner, &game.joiner_entropy),
    )
}

pub fn apply_join(
    game: &mut Game,
    joiner: Pubkey,
    joiner_side: Side,
    joiner_entropy: [u8; 32],
    amount: u64,
) -> Result<(Pubkey, u8)> {
    require!(game.status == GameStatus::Open, CoinflipError::AlreadyJoined);
    require!(amount == game.amount, CoinflipError::AmountMismatch);
    require_nonzero_entropy(&joiner_entropy)?;

    match game.creator_side {
        Side::Open => {
            require!(joiner_side != Side::Open, CoinflipError::InvalidSide);
            game.joiner_side = joiner_side;
            game.creator_side = joiner_side.opposite()?;
        }
        locked => {
            if joiner_side != Side::Open {
                require!(joiner_side != locked, CoinflipError::InvalidSide);
            }
            game.joiner_side = locked.opposite()?;
        }
    }

    game.joiner = joiner;
    game.joiner_entropy = joiner_entropy;
    game.join_slot = Clock::get()?.slot;
    game.status = GameStatus::Ready;
    Ok((game.mint, game.token_decimals))
}
