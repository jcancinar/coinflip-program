use crate::errors::CoinflipError;
use crate::state::{pot_fee, Config, Game, GameStatus, CONFIG_SEED, GAME_SEED};
use crate::token_utils::{
    close_vault_from_game, require_token_program, require_vault_ata, transfer_tokens_from_game,
};
use crate::vrf::{fulfilled_randomness, result_winner, vrf_seed};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Resolve<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        close = winner,
        seeds = [GAME_SEED, game.creator.as_ref(), &game.nonce.to_le_bytes()],
        bump = game.bump
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
        constraint = winner_token.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = winner_token.owner == winner.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub winner_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(
        mut,
        constraint = fee_recipient_token.mint == game.mint @ CoinflipError::InvalidMint,
        constraint = fee_recipient_token.owner == fee_recipient.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub fee_recipient_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// CHECK: must match the computed winner; owner is not required to be System
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
    /// CHECK: owner receives the snapshotted resolve fee
    #[account(
        mut,
        address = config.authority @ CoinflipError::Unauthorized
    )]
    pub fee_recipient: UncheckedAccount<'info>,
    /// CHECK: anyone may settle once ORAO has fulfilled
    pub settler: Signer<'info>,
    /// CHECK: fulfilled ORAO (or configured) randomness account
    pub vrf_request: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Resolve>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(game.status == GameStatus::Ready, CoinflipError::NotReady);

    let seed = vrf_seed(&game.key());
    let randomness = fulfilled_randomness(
        &ctx.accounts.vrf_request.to_account_info(),
        &ctx.accounts.config.vrf_program,
        &seed,
    )?;
    let expected_winner = result_winner(game, &randomness);
    require_keys_eq!(
        ctx.accounts.winner.key(),
        expected_winner,
        CoinflipError::InvalidWinner
    );

    let fee = pot_fee(game.amount, game.fee_bps)?;
    if game.is_native_sol() {
        if fee > 0 {
            let game_info = ctx.accounts.game.to_account_info();
            let fee_info = ctx.accounts.fee_recipient.to_account_info();
            let game_lamports = game_info.lamports();
            let fee_lamports = fee_info.lamports();
            **game_info.try_borrow_mut_lamports()? = game_lamports
                .checked_sub(fee)
                .ok_or(CoinflipError::ArithmeticOverflow)?;
            **fee_info.try_borrow_mut_lamports()? = fee_lamports
                .checked_add(fee)
                .ok_or(CoinflipError::ArithmeticOverflow)?;
        }
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
    let winner_token = ctx
        .accounts
        .winner_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let fee_recipient_token = ctx
        .accounts
        .fee_recipient_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let token_program = require_token_program(&ctx.accounts.token_program)?;
    require_vault_ata(
        &vault.key(),
        &game.key(),
        &game.mint,
        &token_program.key(),
    )?;

    let pot = game
        .amount
        .checked_mul(2)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    let vault_balance = vault.amount;
    require!(vault_balance >= pot, CoinflipError::TokenAccountMismatch);
    let winner_amount = vault_balance
        .checked_sub(fee)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    let creator = game.creator;
    let nonce = game.nonce;
    let bump = game.bump;
    let decimals = game.token_decimals;

    transfer_tokens_from_game(
        token_program.clone(),
        vault.to_account_info(),
        mint_account.to_account_info(),
        fee_recipient_token.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
        fee,
        decimals,
    )?;
    transfer_tokens_from_game(
        token_program.clone(),
        vault.to_account_info(),
        mint_account.to_account_info(),
        winner_token.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
        winner_amount,
        decimals,
    )?;
    close_vault_from_game(
        token_program,
        vault.to_account_info(),
        ctx.accounts.winner.to_account_info(),
        ctx.accounts.game.to_account_info(),
        &creator,
        nonce,
        bump,
    )?;

    Ok(())
}
