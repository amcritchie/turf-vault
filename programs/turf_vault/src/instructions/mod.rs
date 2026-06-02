// One file per instruction. Each module exports a Context<'info> Accounts
// struct and a `handle_*` function. `lib.rs` re-exports thin wrappers under
// `#[program]` that delegate here.
//
// Instructions are grouped roughly by lifecycle:
//   Vault setup & governance — initialize, update_signers
//   Currency registry        — register_currency, deactivate_currency
//   Pause control            — pause, unpause
//   User accounts            — create_user_account, set_username
//   Seasons                  — create_season
//   Contest lifecycle        — create_contest, set_contest_lock_time,
//                              set_contest_conclusion_time, settle_contest,
//                              cancel_contest, close_contest
//   Entries                  — enter_contest, enter_contest_with_token
//   Free entries             — mint_entry_token
//   Treasury                 — sweep_operator_revenue

pub mod initialize;
pub mod register_currency;
pub mod deactivate_currency;
pub mod create_user_account;
pub mod set_username;
pub mod create_season;
pub mod create_contest;
pub mod set_contest_lock_time;
pub mod set_contest_conclusion_time;
pub mod enter_contest;
pub mod enter_contest_with_token;
pub mod settle_contest;
pub mod cancel_contest;
pub mod close_contest;
pub mod mint_entry_token;
pub mod sweep_operator_revenue;
pub mod pause;
pub mod unpause;
pub mod update_signers;

pub use initialize::*;
pub use register_currency::*;
pub use deactivate_currency::*;
pub use create_user_account::*;
pub use set_username::*;
pub use create_season::*;
pub use create_contest::*;
pub use set_contest_lock_time::*;
pub use set_contest_conclusion_time::*;
pub use enter_contest::*;
pub use enter_contest_with_token::*;
pub use settle_contest::*;
pub use cancel_contest::*;
pub use close_contest::*;
pub use mint_entry_token::*;
pub use sweep_operator_revenue::*;
pub use pause::*;
pub use unpause::*;
pub use update_signers::*;
