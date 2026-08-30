use crate::errors::CoinflipError;
use crate::state::{TokenConfig, GAME_SEED, TOKEN_SEED};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::{
    create, get_associated_token_address_with_program_id, AssociatedToken, Create,
};
use anchor_spl::token_interface::{
    close_account, transfer_checked, CloseAccount, TokenInterface, TransferChecked,
};

pub fn require_enabled_token(
    token_config: &Account<TokenConfig>,
    mint: Pubkey,
    amount: u64,
) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(&[TOKEN_SEED, mint.as_ref()], &crate::ID);
    require_keys_eq!(token_config.key(), expected, CoinflipError::InvalidMint);
    require_keys_eq!(token_config.mint, mint, CoinflipError::InvalidMint);
    require!(token_config.is_enabled, CoinflipError::TokenNotEnabled);
    require!(amount >= token_config.min_amount, CoinflipError::AmountBelowMinimum);
    Ok(())
}

pub fn require_vault_ata(
    vault: &Pubkey,
    game: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Result<()> {
    let expected = get_associated_token_address_with_program_id(game, mint, token_program);
    require_keys_eq!(*vault, expected, CoinflipError::TokenAccountMismatch);
    Ok(())
}

pub fn create_ata_if_needed<'info>(
    payer: AccountInfo<'info>,
    vault: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    ata_program: AccountInfo<'info>,
) -> Result<()> {
    if vault.data_is_empty() {
        create(CpiContext::new(
            ata_program,
            Create {
                payer,
                associated_token: vault,
                authority,
                mint,
                system_program,
                token_program,
            },
        ))?;
    }
    Ok(())
}

pub fn transfer_tokens<'info>(
    token_program: AccountInfo<'info>,
    from: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            token_program,
            TransferChecked {
                from,
                mint,
                to,
                authority,
            },
        ),
        amount,
        decimals,
    )
}

pub fn transfer_tokens_from_game<'info>(
    token_program: AccountInfo<'info>,
    from: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    game: AccountInfo<'info>,
    creator: &Pubkey,
    nonce: u64,
    bump: u8,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let nonce_bytes = nonce.to_le_bytes();
    let seeds: &[&[u8]] = &[GAME_SEED, creator.as_ref(), &nonce_bytes, &[bump]];
    transfer_checked(
        CpiContext::new_with_signer(
            token_program,
            TransferChecked {
                from,
                mint,
                to,
                authority: game,
            },
            &[seeds],
        ),
        amount,
        decimals,
    )
}

pub fn close_vault_from_game<'info>(
    token_program: AccountInfo<'info>,
    vault: AccountInfo<'info>,
    destination: AccountInfo<'info>,
    game: AccountInfo<'info>,
    creator: &Pubkey,
    nonce: u64,
    bump: u8,
) -> Result<()> {
    let nonce_bytes = nonce.to_le_bytes();
    let seeds: &[&[u8]] = &[GAME_SEED, creator.as_ref(), &nonce_bytes, &[bump]];
    close_account(CpiContext::new_with_signer(
        token_program,
        CloseAccount {
            account: vault,
            destination,
            authority: game,
        },
        &[seeds],
    ))
}

pub fn require_token_program<'info>(
    token_program: &Option<Interface<'info, TokenInterface>>,
) -> Result<AccountInfo<'info>> {
    Ok(token_program
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?
        .to_account_info())
}

pub fn require_ata_program<'info>(
    ata_program: &Option<Program<'info, AssociatedToken>>,
) -> Result<AccountInfo<'info>> {
    Ok(ata_program
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?
        .to_account_info())
}
