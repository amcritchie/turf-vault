use anchor_lang::prelude::*;

pub mod errors;
pub mod state;
pub mod instructions;

use instructions::*;

declare_id!("Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT");

#[program]
pub mod turf_vault {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, signers: [Pubkey; 3], threshold: u8) -> Result<()> {
        handle_initialize(ctx, signers, threshold)
    }

    pub fn force_close_vault(ctx: Context<ForceCloseVault>) -> Result<()> {
        handle_force_close_vault(ctx)
    }

    pub fn create_user_account(
        ctx: Context<CreateUserAccount>,
        wallet: Pubkey,
        username: [u8; 32],
    ) -> Result<()> {
        handle_create_user_account(ctx, wallet, username)
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        handle_deposit(ctx, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        handle_withdraw(ctx, amount)
    }

    pub fn create_contest(
        ctx: Context<CreateContest>,
        contest_id: [u8; 32],
        season_id: u32,
        entry_fee: u64,
        max_entries: u32,
        payout_amounts: Vec<u64>,
        prizes: u64,
    ) -> Result<()> {
        handle_create_contest(ctx, contest_id, season_id, entry_fee, max_entries, payout_amounts, prizes)
    }

    pub fn enter_contest(ctx: Context<EnterContest>, entry_num: u32) -> Result<()> {
        handle_enter_contest(ctx, entry_num)
    }

    pub fn enter_contest_direct(ctx: Context<EnterContestDirect>, entry_num: u32) -> Result<()> {
        handle_enter_contest_direct(ctx, entry_num)
    }

    pub fn settle_contest(ctx: Context<SettleContest>, settlements: Vec<Settlement>) -> Result<()> {
        handle_settle_contest(ctx, settlements)
    }

    pub fn close_contest(ctx: Context<CloseContest>) -> Result<()> {
        handle_close_contest(ctx)
    }

    pub fn migrate_user_account(ctx: Context<MigrateUserAccount>) -> Result<()> {
        handle_migrate_user_account(ctx)
    }

    pub fn update_signers(ctx: Context<UpdateSigners>, new_signers: [Pubkey; 3], new_threshold: u8) -> Result<()> {
        handle_update_signers(ctx, new_signers, new_threshold)
    }

    pub fn mint_entry_token(
        ctx: Context<MintEntryToken>,
        sequence: u64,
        source: u8,
        source_ref: [u8; 64],
    ) -> Result<()> {
        handle_mint_entry_token(ctx, sequence, source, source_ref)
    }

    pub fn enter_contest_with_token(
        ctx: Context<EnterContestWithToken>,
        entry_num: u32,
    ) -> Result<()> {
        handle_enter_contest_with_token(ctx, entry_num)
    }

    pub fn enter_contest_direct_with_token(
        ctx: Context<EnterContestDirectWithToken>,
        entry_num: u32,
    ) -> Result<()> {
        handle_enter_contest_direct_with_token(ctx, entry_num)
    }

    pub fn create_season(
        ctx: Context<CreateSeason>,
        season_id: u32,
        name: [u8; 32],
        seed_schedule: [u64; 5],
        start_at: i64,
    ) -> Result<()> {
        handle_create_season(ctx, season_id, name, seed_schedule, start_at)
    }

    pub fn set_username(ctx: Context<SetUsername>, username: [u8; 32]) -> Result<()> {
        handle_set_username(ctx, username)
    }
}
