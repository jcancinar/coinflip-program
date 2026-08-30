use crate::errors::CoinflipError;
use crate::state::{
    Config, Game, GameStatus, AUTHORITY_CANCEL_DELAY_SLOTS, CONFIG_SEED, GAME_SEED,
};
use crate::token_utils::{
    close_vault_from_game, require_token_program, require_vault_ata, transfer_tokens_from_game,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Cancel<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        close = creator,
        seeds = [GAME_SEED, game.creator.as_ref(), &game.nonce.to_le_bytes()],
        bump = game.bump,
        constraint = creator.key() == game.creator @ CoinflipError::Unauthorized
    )]
    pub game: Account<'info, Game>,
    pub mint_account: Option<Box<InterfaceAccount<'info, Mint>>>,
    #[account(
        mut,
        constraint = vault.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = vault.owner == game.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub vault: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(
        mut,
        constraint = creator_token.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = creator_token.owner == creator.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub creator_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// CHECK: refund destination; constrained to game.creator above
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct InitiateCancel<'info> {
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
    pub authority: Signer<'info>,
}

pub fn initiate(ctx: Context<InitiateCancel>) -> Result<()> {
    require_keys_eq!(
        ctx.accounts.authority.key(),
        ctx.accounts.config.authority,
        CoinflipError::Unauthorized
    );
    require!(
        ctx.accounts.game.status == GameStatus::Open,
        CoinflipError::CancelNotAllowed
    );
    let now = Clock::get()?.slot;
    ctx.accounts.game.cancel_after_slot = now
        .checked_add(AUTHORITY_CANCEL_DELAY_SLOTS)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    Ok(())
}

pub fn handler(ctx: Context<Cancel>) -> Result<()> {
    require!(
        ctx.accounts.game.status == GameStatus::Open,
        CoinflipError::CancelNotAllowed
    );

    let signer = ctx.accounts.signer.key();
    let creator = ctx.accounts.game.creator;
    let authority = ctx.accounts.config.authority;
    require!(signer == creator || signer == authority, CoinflipError::Unauthorized);

    if signer == authority && signer != creator {
        require!(
            ctx.accounts.game.cancel_after_slot > 0,
            CoinflipError::AuthorityCancelNotInitiated
        );
        require!(
            Clock::get()?.slot >= ctx.accounts.game.cancel_after_slot,
            CoinflipError::AuthorityCancelNotReady
        );
    }

    if ctx.accounts.game.is_native_sol() {
        return Ok(());
    }

    let game = &ctx.accounts.game;
    let mint_account = ctx
        .accounts
        .mint_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(mint_account.key(), game.mint, CoinflipError::InvalidMint);
    let vault = ctx
        .accounts
        .vault
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let creator_token = ctx
        .accounts
        .creator_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let token_program = require_token_program(&ctx.accounts.token_program)?;
    require_vault_ata(
        &vault.key(),
        &game.key(),
        &game.mint,
        &token_program.key(),
    )?;

    let vault_balance = vault.amount;
    require!(vault_balance >= game.amount, CoinflipError::TokenAccountMismatch);
    let creator = game.creator;
    let nonce = game.nonce;
    let bump = game.bump;
    let decimals = game.token_decimals;

    transfer_tokens_from_game(
        token_program.clone(),
        vault.to_account_info(),
        mint_account.to_account_info(),
        creator_token.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
        vault_balance,
        decimals,
    )?;
    close_vault_from_game(
        token_program,
        vault.to_account_info(),
        ctx.accounts.creator.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
    )?;

    Ok(())
}
