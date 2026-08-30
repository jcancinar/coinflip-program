use crate::errors::CoinflipError;
use crate::state::{
    is_native_sol, require_nonzero_entropy, Config, Game, GameStatus, Side, TokenConfig,
    CONFIG_SEED, GAME_SEED,
};
use crate::token_utils::{
    create_ata_if_needed, require_ata_program, require_enabled_token, require_token_program,
    require_vault_ata, transfer_tokens,
};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
#[instruction(amount: u64, side: Side, creator_entropy: [u8; 32], nonce: u64, mint: Pubkey)]
pub struct Create<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        init,
        payer = creator,
        space = 8 + Game::INIT_SPACE,
        seeds = [GAME_SEED, creator.key().as_ref(), &nonce.to_le_bytes()],
        bump
    )]
    pub game: Account<'info, Game>,
    pub token_config: Option<Box<Account<'info, TokenConfig>>>,
    pub mint_account: Option<Box<InterfaceAccount<'info, Mint>>>,
    #[account(
        mut,
        constraint = creator_token.mint == mint @ CoinflipError::InvalidMint,
        constraint = creator_token.owner == creator.key() @ CoinflipError::TokenAccountMismatch
    )]
    pub creator_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    /// CHECK: ATA of the game PDA; created if needed
    #[account(mut)]
    pub vault: Option<UncheckedAccount<'info>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<Create>,
    amount: u64,
    side: Side,
    creator_entropy: [u8; 32],
    nonce: u64,
    mint: Pubkey,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, CoinflipError::Paused);
    require!(amount > 0, CoinflipError::InvalidAmount);
    require_nonzero_entropy(&creator_entropy)?;

    let token_decimals = if is_native_sol(&mint) {
        0
    } else {
        let token_config = ctx
            .accounts
            .token_config
            .as_ref()
            .ok_or(CoinflipError::TokenNotEnabled)?;
        require_enabled_token(token_config, mint, amount)?;

        let mint_account = ctx
            .accounts
            .mint_account
            .as_ref()
            .ok_or(CoinflipError::TokenAccountRequired)?;
        require_keys_eq!(mint_account.key(), mint, CoinflipError::InvalidMint);
        mint_account.decimals
    };

    {
        let game = &mut ctx.accounts.game;
        game.creator = ctx.accounts.creator.key();
        game.joiner = Pubkey::default();
        game.amount = amount;
        game.mint = mint;
        game.token_decimals = token_decimals;
        game.fee_bps = ctx.accounts.config.fee_bps;
        game.creator_side = side;
        game.joiner_side = Side::Open;
        game.creator_entropy = creator_entropy;
        game.joiner_entropy = [0u8; 32];
        game.commit = [0u8; 32];
        game.status = GameStatus::Open;
        game.nonce = nonce;
        game.bump = ctx.bumps.game;
    }

    if is_native_sol(&mint) {
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.creator.to_account_info(),
                    to: ctx.accounts.game.to_account_info(),
                },
            ),
            amount,
        )?;
        return Ok(());
    }

    let mint_account = ctx
        .accounts
        .mint_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let creator_token = ctx
        .accounts
        .creator_token
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let vault = ctx
        .accounts
        .vault
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    let token_program = require_token_program(&ctx.accounts.token_program)?;
    let ata_program = require_ata_program(&ctx.accounts.associated_token_program)?;

    require_vault_ata(
        &vault.key(),
        &ctx.accounts.game.key(),
        &mint,
        &token_program.key(),
    )?;

    create_ata_if_needed(
        ctx.accounts.creator.to_account_info(),
        vault.to_account_info(),
        ctx.accounts.game.to_account_info(),
        mint_account.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        token_program.clone(),
        ata_program,
    )?;

    transfer_tokens(
        token_program,
        creator_token.to_account_info(),
        mint_account.to_account_info(),
        vault.to_account_info(),
        ctx.accounts.creator.to_account_info(),
        amount,
        token_decimals,
    )?;

    Ok(())
}
