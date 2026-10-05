//! Rust client for the EcoCash Open API (charge, lookup, refund/reversal).
//!
//! ```no_run
//! # async fn run() -> ecocash::Result<()> {
//! use ecocash::{ChargeRequest, Ecocash, MerchantConfig, PollOptions, PollStrategy};
//!
//! let merchant = MerchantConfig::new("001535", "1234", "788732685", "UAT STORE 3", "UAT00003");
//! let client = Ecocash::builder("<user>", "<password>", merchant).build()?;
//! let charge = client
//!     .charge(&ChargeRequest { phone: "773047653".into(), amount: 2.0, ..Default::default() })
//!     .await?;
//! let tx = client
//!     .poll_transaction(&charge, PollStrategy::Backoff, PollOptions::default())
//!     .await?;
//! println!("paid: {}", tx.is_success());
//! # Ok(()) }
//! ```

mod client;
mod error;
mod types;

pub use client::{Ecocash, EcocashBuilder, SANDBOX_BASE_URL};
pub use error::{Error, ErrorCode, Result};
pub use types::*;
