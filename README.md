# ecocash

Rust client for the [EcoCash](https://developers.ecocash.co.zw) Open API: merchant charge (USSD PIN prompt), transaction lookup, and refund/reversal. Ported from the TypeScript `ecocash` SDK and updated to the v1 Basic-Auth API.

```rust
use ecocash::{ChargeRequest, Ecocash, MerchantConfig, PollOptions, PollStrategy, RefundRequest};

#[tokio::main]
async fn main() -> ecocash::Result<()> {
    let merchant = MerchantConfig::sandbox(); // shared sandbox merchant 001535
    // or Ecocash::builder_encoded("<base64 credentials>", merchant)
    // sandbox by default; `.base_url(..)` points it at production
    let client = Ecocash::builder("<user>", "<password>", merchant).build()?;

    let charge = client
        .charge(&ChargeRequest { phone: "773047653".into(), amount: 2.0, ..Default::default() })
        .await?;

    // Simple (no delay), Interval (fixed delay) or Backoff (delay *= multiplier)
    let tx = client
        .poll_transaction(&charge, PollStrategy::Backoff,
            PollOptions { multiplier: 2, interval: 15, sleep: 2000 })
        .await?;

    if tx.is_success() {
        let id = tx.transaction_id.clone().unwrap();
        // RefundRequest::reversal(..) for tranType REV
        let r = client.refund(&RefundRequest::refund("773047653", 2.0, id)).await?;
        println!("{:?}", r.status);
    }
    Ok(())
}
```

Polling stops as soon as the status is no longer `PENDING*`; check `is_success()`. Errors: `Error::Api { status, body }` for non-2xx responses.

Sandbox smoke test: set the `ECOCASH_*` variables listed in `examples/sandbox.rs` and run `cargo run --example sandbox`.

MIT licensed.

## Errors

Non-2xx responses become `Error::Api { status, code, message, body }`. `code` is the documented `ErrorCode` (E001-E015) parsed from `statusCode`; use `err.code()`, `ErrorCode::hint()` for the resolution hint, and `err.is_retryable()` (transport failures, 5xx, E014, E015).

## Sandbox testing

Whitelist and OTP-verify your own MSISDN on the developer portal's Test Numbers page (`263XXXXXXXXX` or `07XXXXXXXX`), charge it, then enter a PIN at the USSD prompt: `0000` success, `1111` insufficient funds, `2222` invalid PIN, `9999` limit exceeded. The sandbox answers all of these with HTTP 200; `TransactionResponse::outcome()` classifies `statusMessage`. A reused `clientCorrelator` returns the existing transaction, so keep correlators unique.
