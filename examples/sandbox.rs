//! Live sandbox smoke test. Needs ECOCASH_USER + ECOCASH_PASS (or ECOCASH_BASIC)
//! and ECOCASH_PHONE (a whitelisted, OTP-verified test number); see common.rs for
//! merchant overrides. Optional: ECOCASH_AMOUNT, ECOCASH_REFUND=1.
//! Enter a PIN from the matrix on the phone's USSD prompt after the charge is sent.
#[path = "common/mod.rs"]
mod common;

use ecocash::{ChargeRequest, PollOptions, PollStrategy, RefundRequest};

#[tokio::main]
async fn main() -> ecocash::Result<()> {
    let client = common::client();
    let phone = common::env("ECOCASH_PHONE");
    let amount = common::opt("ECOCASH_AMOUNT")
        .and_then(|a| a.parse().ok())
        .unwrap_or(2.0);

    let charge = client
        .charge(&ChargeRequest {
            phone: phone.clone(),
            amount,
            ..Default::default()
        })
        .await?;
    println!("charge: {charge:#?}");
    let tx = client
        .poll_transaction(
            &charge,
            PollStrategy::Interval,
            PollOptions {
                sleep: 2000,
                interval: 15,
                ..Default::default()
            },
        )
        .await?;
    println!("final: {tx:#?}");

    if tx.is_success() && common::opt("ECOCASH_REFUND").is_some() {
        let id = tx.transaction_id.clone().expect("transactionId");
        println!(
            "refund: {:#?}",
            client
                .refund(&RefundRequest::refund(phone, amount, id))
                .await?
        );
    }
    Ok(())
}
