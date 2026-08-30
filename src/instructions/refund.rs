use crate::errors::CoinflipError;
use crate::slot_hash::{lookup_join_slot_hash, SlotHashLookup};
use crate::vrf::{is_request_fulfilled, randomness_address, vrf_seed};
use crate::state::{Config, Game, GameStatus, CONFIG_SEED, GAME_SEED};
use crate::token_utils::{
    close_vault_from_game, require_token_program, require_vault_ata, transfer_tokens_from_game,
};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::slot_hashes::ID as SLOT_HASHES_ID;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct RefundExpired<'info> {
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
    #[account(
        mut,
        constraint = joiner_token.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = joiner_token.owner == joiner.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub joiner_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// CHECK: must be the game creator
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: must be the game joiner
    #[account(mut, address = game.joiner @ CoinflipError::Unauthorized)]
    pub joiner: UncheckedAccount<'info>,
    /// CHECK: SlotHashes sysvar
    #[account(address = SLOT_HASHES_ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    /// CHECK: must be this game's VRF request PDA; refund is rejected if fulfilled
    #[account(
        constraint = vrf_request.key()
            == randomness_address(
                &config.vrf_program,
                &vrf_seed(&game.key(), &game.joiner, &game.joiner_entropy)
            )
            @ CoinflipError::InvalidVrfAccounts
    )]
    pub vrf_request: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<RefundExpired>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(game.status == GameStatus::Ready, CoinflipError::NotReady);
    require!(game.join_slot > 0, CoinflipError::NotReady);
    let seed = vrf_seed(&game.key(), &game.joiner, &game.joiner_entropy);
    require!(
        !is_request_fulfilled(
            &ctx.accounts.vrf_request.to_account_info(),
            &ctx.accounts.config.vrf_program,
            &seed
        )?,
        CoinflipError::CancelNotAllowed
    );
    match lookup_join_slot_hash(&ctx.accounts.slot_hashes.to_account_info(), game.join_slot)? {
        SlotHashLookup::Expired => {}
        SlotHashLookup::Found(_) => return err!(CoinflipError::CancelNotAllowed),
        SlotHashLookup::NotReady => return err!(CoinflipError::SlotHashNotReady),
    }

    if game.is_native_sol() {
        let amount = game.amount;
        let game_info = ctx.accounts.game.to_account_info();
        let joiner_info = ctx.accounts.joiner.to_account_info();
        let game_lamports = game_info.lamports();
        let joiner_lamports = joiner_info.lamports();
        **game_info.try_borrow_mut_lamports()? = game_lamports
            .checked_sub(amount)
            .ok_or(CoinflipError::ArithmeticOverflow)?;
        **joiner_info.try_borrow_mut_lamports()? = joiner_lamports
            .checked_add(amount)
            .ok_or(CoinflipError::ArithmeticOverflow)?;
        return Ok(());
    }

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
    let joiner_token = ctx
        .accounts
        .joiner_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let token_program = require_token_program(&ctx.accounts.token_program)?;
    require_vault_ata(
        &vault.key(),
        &game.key(),
        &game.mint,
        &token_program.key(),
    )?;

    let stake = game.amount;
    let vault_balance = vault.amount;
    require!(vault_balance >= stake.saturating_mul(2), CoinflipError::TokenAccountMismatch);
    let creator = game.creator;
    let nonce = game.nonce;
    let bump = game.bump;
    let decimals = game.token_decimals;
    let creator_amount = vault_balance.saturating_sub(stake);

    transfer_tokens_from_game(
        token_program.clone(),
        vault.to_account_info(),
        mint_account.to_account_info(),
        joiner_token.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
        stake,
        decimals,
    )?;
    transfer_tokens_from_game(
        token_program.clone(),
        vault.to_account_info(),
        mint_account.to_account_info(),
        creator_token.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
        creator_amount,
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
