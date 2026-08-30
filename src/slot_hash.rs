use crate::errors::CoinflipError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::slot_hashes::ID as SLOT_HASHES_ID;

pub enum SlotHashLookup {
    Found([u8; 32]),
    NotReady,
    Expired,
}

/// SlotHashes sysvar: u64 count, then newest-first (u64 slot, [u8;32] hash).
pub fn lookup_join_slot_hash(slot_hashes: &AccountInfo, join_slot: u64) -> Result<SlotHashLookup> {
    require_keys_eq!(*slot_hashes.key, SLOT_HASHES_ID, CoinflipError::SlotHashNotReady);
    let data = slot_hashes.try_borrow_data()?;
    require!(data.len() >= 8, CoinflipError::SlotHashNotReady);
    let n = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;
    let mut off = 8;
    let mut oldest = join_slot;
    for _ in 0..n {
        require!(off + 40 <= data.len(), CoinflipError::SlotHashNotReady);
        let slot = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        let hash: [u8; 32] = data[off + 8..off + 40].try_into().unwrap();
        if slot == join_slot {
            return Ok(SlotHashLookup::Found(hash));
        }
        oldest = slot;
        off += 40;
    }
    let clock = Clock::get()?;
    if clock.slot <= join_slot || (n > 0 && oldest <= join_slot) {
        return Ok(SlotHashLookup::NotReady);
    }
    Ok(SlotHashLookup::Expired)
}
