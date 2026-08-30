use crate::errors::CoinflipError;
use crate::instructions::join::apply_join;
use crate::state::{
    effective_mint, is_sol_mint, Config, Game, Side, TokenConfig, CONFIG_SEED, GAME_SEED, WSOL_MINT,
};
use crate::state::TOKEN_PROGRAM_ID;
use crate::swap::{split_hops, swap_exact_out, token_amount};
use crate::token_utils::{require_token_program, require_vault_ata, transfer_tokens};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    close_account, sync_native, CloseAccount, Mint, SyncNative, TokenAccount, TokenInterface,
};

#[derive(Accounts)]
pub struct Participate<'info> {
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
    pub game_token_config: Option<Account<'info, TokenConfig>>,
    pub mint_account: Option<Box<InterfaceAccount<'info, Mint>>>,
    #[account(mut)]
    pub joiner_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(mut)]
    pub joiner_pay_token: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(mut)]
    pub vault: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    #[account(mut)]
    pub wsol_account: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Option<Interface<'info, TokenInterface>>,
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// CHECK: Raydium CLMM (mainnet or devnet) when a swap is required
    pub raydium_program: Option<UncheckedAccount<'info>>,
    #[account(
        mut,
        constraint = joiner.key() != game.creator @ CoinflipError::CannotJoinOwnGame
    )]
    pub joiner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, Participate<'info>>,
    joiner_side: Side,
    joiner_entropy: [u8; 32],
    amount: u64,
    pay_mint: Pubkey,
    max_pay: u64,
    quote_out: u64,
    hop1_len: u8,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, CoinflipError::Paused);

    let game_mint = ctx.accounts.game.mint;
    if effective_mint(&game_mint) != effective_mint(&pay_mint) {
        run_swaps(&ctx, &game_mint, &pay_mint, amount, max_pay, quote_out, hop1_len)?;
        if is_sol_mint(&game_mint) {
            unwrap_wsol_to_joiner(&ctx)?;
        }
    } else {
        require!(max_pay >= amount, CoinflipError::InvalidAmount);
    }

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
        return Ok(());
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

    Ok(())
}

fn run_swaps<'info>(
    ctx: &Context<'_, '_, 'info, 'info, Participate<'info>>,
    game_mint: &Pubkey,
    pay_mint: &Pubkey,
    amount_out: u64,
    max_pay: u64,
    quote_out: u64,
    hop1_len: u8,
) -> Result<()> {
    require!(max_pay > 0, CoinflipError::InvalidAmount);
    let remaining = ctx.remaining_accounts;
    let clmm = ctx
        .accounts
        .raydium_program
        .as_ref()
        .ok_or(CoinflipError::InvalidSwapAccounts)?
        .to_account_info();
    let owner = ctx.accounts.joiner.to_account_info();
    let (first_pool, second_pool) = allowed_pools(
        &ctx.accounts.config,
        ctx.accounts.game_token_config.as_ref(),
        game_mint,
        pay_mint,
    )?;

    if is_sol_mint(pay_mint) {
        wrap_sol(ctx, remaining, max_pay)?;
    }

    if let Some(second_pool) = second_pool {
        require!(quote_out > 0 && hop1_len > 0, CoinflipError::InvalidSwapAccounts);
        let (hop1, hop2) = split_hops(remaining, hop1_len)?;
        swap_exact_out(clmm.clone(), owner.clone(), hop1, &first_pool, max_pay, quote_out)?;
        swap_exact_out(clmm, owner, hop2, &second_pool, token_amount(&hop2[2])?, amount_out)
    } else {
        let (hop, rest) = split_hops(remaining, 0)?;
        require!(rest.is_empty(), CoinflipError::InvalidSwapAccounts);
        swap_exact_out(clmm, owner, hop, &first_pool, max_pay, amount_out)
    }
}

fn allowed_pools(
    config: &Config,
    token_config: Option<&Account<TokenConfig>>,
    game_mint: &Pubkey,
    pay_mint: &Pubkey,
) -> Result<(Pubkey, Option<Pubkey>)> {
    let sol_usdc = config.sol_usdc_pool;
    let usdc = config.usdc_mint;

    if is_sol_mint(game_mint) {
        require!(usdc != Pubkey::default() && *pay_mint == usdc, CoinflipError::InvalidPayMint);
        require!(sol_usdc != Pubkey::default(), CoinflipError::SolUsdcPoolNotSet);
        return Ok((sol_usdc, None));
    }

    if usdc != Pubkey::default() && *game_mint == usdc && is_sol_mint(pay_mint) {
        require!(sol_usdc != Pubkey::default(), CoinflipError::SolUsdcPoolNotSet);
        return Ok((sol_usdc, None));
    }

    let token_config = token_config.ok_or(CoinflipError::TokenNotEnabled)?;
    require!(token_config.is_enabled, CoinflipError::TokenNotEnabled);
    require_keys_eq!(token_config.mint, *game_mint, CoinflipError::InvalidMint);
    require!(!token_config.cross_disabled, CoinflipError::InvalidPayMint);
    require!(token_config.pool != Pubkey::default(), CoinflipError::PoolRequired);

    if effective_mint(pay_mint) == effective_mint(&token_config.quote_mint) {
        return Ok((token_config.pool, None));
    }

    let via_usdc = *pay_mint == usdc && is_sol_mint(&token_config.quote_mint);
    let via_sol = is_sol_mint(pay_mint) && token_config.quote_mint == usdc;
    require!(via_usdc || via_sol, CoinflipError::InvalidPayMint);
    require!(sol_usdc != Pubkey::default(), CoinflipError::SolUsdcPoolNotSet);
    Ok((sol_usdc, Some(token_config.pool)))
}

fn wrap_sol<'info>(
    ctx: &Context<'_, '_, 'info, 'info, Participate<'info>>,
    remaining: &[AccountInfo<'info>],
    lamports: u64,
) -> Result<()> {
    let wsol = ctx
        .accounts
        .wsol_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(wsol.mint, WSOL_MINT, CoinflipError::InvalidMint);
    require_keys_eq!(
        wsol.owner,
        ctx.accounts.joiner.key(),
        CoinflipError::TokenAccountMismatch
    );
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.joiner.to_account_info(),
                to: wsol.to_account_info(),
            },
        ),
        lamports,
    )?;
    sync_native(CpiContext::new(
        tokenkeg(ctx, remaining)?,
        SyncNative {
            account: wsol.to_account_info(),
        },
    ))
}

fn unwrap_wsol_to_joiner<'info>(
    ctx: &Context<'_, '_, 'info, 'info, Participate<'info>>,
) -> Result<()> {
    let wsol = ctx
        .accounts
        .wsol_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(wsol.mint, WSOL_MINT, CoinflipError::InvalidMint);
    close_account(CpiContext::new(
        tokenkeg(ctx, ctx.remaining_accounts)?,
        CloseAccount {
            account: wsol.to_account_info(),
            destination: ctx.accounts.joiner.to_account_info(),
            authority: ctx.accounts.joiner.to_account_info(),
        },
    ))
}

fn tokenkeg<'info>(
    ctx: &Context<'_, '_, 'info, 'info, Participate<'info>>,
    remaining: &[AccountInfo<'info>],
) -> Result<AccountInfo<'info>> {
    if let Some(program) = &ctx.accounts.token_program {
        if program.key() == TOKEN_PROGRAM_ID {
            return Ok(program.to_account_info());
        }
    }
    remaining
        .iter()
        .find(|account| account.key() == TOKEN_PROGRAM_ID)
        .cloned()
        .ok_or(error!(CoinflipError::TokenAccountRequired))
}
