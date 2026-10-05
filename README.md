# ecocash

An async Rust client for the [EcoCash](https://developers.ecocash.co.zw) Open API (Zimbabwe mobile money): push a payment prompt to a customer's phone, look the transaction up, poll until it settles, and refund or reverse it.

It started as a port of the TypeScript [`ecocash`](https://github.com/phoscoder/ecocash) SDK and was then rebuilt against the newer v1 "Instant Payment" API (HTTP Basic Auth).

> [!WARNING]
> **This crate is not complete and has not been verified against the live sandbox.**
>
> Requests to the documented sandbox endpoints currently return `401 Unauthorized` for the issued sandbox credentials, even though the same credentials, URL and `Authorization: Basic` scheme match the portal's own documentation and JavaScript example. **An issue with EcoCash's developers is pending resolution.** Until it is resolved:
>
> - everything here is implemented from the published API reference and is covered only by mock-server tests, not by real EcoCash responses;
> - response field names and statuses (`status` vs `transactionStatus`, the final state after a failed PIN, the shape of error bodies, whether `originalEcocashReference` is accepted on refunds) are assumptions that may need adjusting;
> - the API is unstable and may change before a first release;
> - do not use it for production payments.
>
> The crate is not published on crates.io. Track progress in this repository's commits and issues.

## Features

- **Charge** a customer (`MER`): the customer receives a USSD PIN prompt.
- **Look up** a transaction by customer MSISDN and `clientCorrelator`.
- **Poll** with a choice of strategy: no delay, fixed interval, or exponential backoff.
- **Refund or reverse** (`REF` / `REV`) a completed charge.
- **Typed errors** for the documented `E001`-`E015` codes, with a resolution hint and a retryability check.
- **Lenient responses**: every response field is optional and unknown fields are preserved in `extra`, because the API schema is only loosely documented.
- Built on `reqwest` (rustls, no OpenSSL) and `tokio`.

## Installation

The crate is not on crates.io yet. Use it from Git:

```toml
[dependencies]
ecocash = { git = "https://github.com/phoscoder/ecocash-rust" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use ecocash::{ChargeRequest, Ecocash, MerchantConfig, PollOptions, PollStrategy, RefundRequest};

#[tokio::main]
async fn main() -> ecocash::Result<()> {
    // Merchant details issued by EcoCash.
    let merchant = MerchantConfig::new(
        "287164",        // merchantCode
        "1234",          // merchantPin
        "778503033",     // merchantNumber (the merchant's own MSISDN)
        "Test Merchant", // merchantName
        "TERM001",       // terminalID
    );

    // Basic Auth from username and password (sandbox base URL by default).
    let client = Ecocash::builder("<username>", "<password>", merchant).build()?;

    // 1. Charge: the customer gets a USSD prompt and enters their PIN.
    let charge = client
        .charge(&ChargeRequest {
            phone: "778548832".into(),
            amount: 2.0,
            ..Default::default()
        })
        .await?;

    // 2. Poll until the status is no longer PENDING.
    let tx = client
        .poll_transaction(&charge, PollStrategy::Backoff, PollOptions::default())
        .await?;

    // 3. Refund if it succeeded.
    if tx.is_success() {
        let original = tx.transaction_id.clone().expect("transactionId");
        let refund = client
            .refund(&RefundRequest::refund("778548832", 2.0, original))
            .await?;
        println!("refund: {}", refund.state());
    }
    Ok(())
}
```

## Configuration

```rust
use std::time::Duration;

let client = Ecocash::builder("<username>", "<password>", merchant)
    .base_url("https://developers.ecocash.co.zw/sandbox/payment") // the default
    .timeout(Duration::from_secs(30))                              // the default
    .build()?;

// Or, if you already have the base64 of "username:password":
let client = Ecocash::builder_encoded("<base64 credentials>", merchant).build()?;
```

| Setting | Default | Notes |
| --- | --- | --- |
| Base URL | `https://developers.ecocash.co.zw/sandbox/payment` | Exposed as `SANDBOX_BASE_URL`. Set `.base_url(..)` for production. The `v1/` segment is added to each endpoint path. |
| Timeout | 30 s | Per request. |
| Auth | HTTP Basic | `Authorization: Basic base64(username:password)`. |

`MerchantConfig::new` defaults `countryCode` to `ZW`, `superMerchantName` to `ECOCASH`, `channel` to `POS` and `location` to `Harare`. All fields are public, so override what you need:

```rust
let mut merchant = MerchantConfig::new("287164", "1234", "778503033", "Test Merchant", "TERM001");
merchant.channel = "WEB".into();
merchant.super_merchant_name = "EcoCash Sandbox".into();
```

`MerchantConfig::sandbox()` returns the shared sandbox merchant (`001535`, `UAT STORE 3`).

## Usage

### Charge

```rust
let charge = client
    .charge(&ChargeRequest {
        phone: "778548832".into(),        // customer MSISDN
        amount: 5.0,
        currency: Some("USD".into()),     // default USD; the API accepts USD or ZWG
        description: Some("Invoice 42".into()),
        remarks: Some("Online payment".into()),
        notify_url: Some("https://example.com/webhook".into()),
        client_correlator: None,          // generated (UUID) if None
        reference_code: None,             // generated if None
    })
    .await?;
```

Each `clientCorrelator` must be unique; reusing one returns the existing transaction. The returned response always carries `client_correlator` and `end_user_id` so it can be passed straight to polling.

### Look up

```rust
let tx = client.lookup_transaction("778548832", &correlator).await?;
println!("{} {:?}", tx.state(), tx.transaction_id);
```

### Poll

```rust
use ecocash::{PollOptions, PollStrategy};

let tx = client
    .poll_transaction(
        &charge,
        PollStrategy::Backoff, // Simple | Interval | Backoff
        PollOptions { multiplier: 2, sleep: 2000, interval: 15 },
    )
    .await?;
```

| Strategy | Behaviour |
| --- | --- |
| `Simple` | Look up repeatedly with no delay. |
| `Interval` | Wait `sleep` ms between lookups. |
| `Backoff` | Wait `sleep` ms, multiplying by `multiplier` after each lookup. |

`interval` is the **maximum number of lookups**, not a delay. Defaults are `multiplier: 2`, `sleep: 1000`, `interval: 10`; zero values fall back to these. Polling stops as soon as the state is no longer `PENDING*` (so also on `FAILED`) and returns the last response if attempts run out, so always check `is_success()` / `is_failed()` / `is_pending()`.

### Refund or reverse

```rust
let refund = client
    .refund(&RefundRequest::refund("778548832", 2.0, original_transaction_id))
    .await?;

// A merchant reversal instead of a customer refund:
let reversal = client
    .refund(&RefundRequest::reversal("778548832", 2.0, original_transaction_id))
    .await?;
```

The amount must not exceed the original charge (otherwise `E012`).

### Reading responses

All three calls return a `TransactionResponse`:

| Field / method | Meaning |
| --- | --- |
| `state()` | `transactionStatus` if present, else `status` (`PENDING`, `SUCCESS`, `FAILED`, ...). |
| `is_success()` / `is_pending()` / `is_failed()` | Convenience checks on `state()`. |
| `outcome()` | Classifies `statusMessage` into `Outcome::{Successful, InsufficientBalance, InvalidPin, LimitExceeded, Other}`. |
| `transaction_id`, `client_correlator`, `status_code`, `status_message`, `amount`, `currency`, `end_user_id`, `merchant_code`, `reference_code`, `original_reference`, `timestamp`, `description` | Documented fields, all `Option`. |
| `extra` | Any other fields the server returned. |

An accepted charge reports `statusMessage: "Transaction Successful"` while its state is still `PENDING`. Use `is_success()` for the final payment state, not `outcome()`.

## Error handling

Non-2xx responses become `Error::Api { status, code, message, body }`:

```rust
use ecocash::{Error, ErrorCode};

match client.charge(&request).await {
    Ok(charge) => { /* ... */ }
    Err(err) => {
        if let Some(code) = err.code() {
            eprintln!("{code}: {}", code.hint());
        }
        if err.is_retryable() {
            // transport failure, any 5xx, E014 or E015: retry with backoff
        }
    }
}
```

`code` is parsed from the `statusCode` field of the error body when it is a documented code, otherwise `None` (the raw `body` is always kept). Other variants: `Error::Http` (network, TLS, timeout), `Error::Json` (unparseable body) and `Error::MissingField` (e.g. polling a response without a correlator).

| Code | HTTP | Meaning |
| --- | --- | --- |
| E001 | 400 | Missing required field |
| E002 | 400 | Invalid MSISDN format (e.g. `773047653`) |
| E003 | 400 | Invalid currency (`USD` or `ZWG`) |
| E004 | 400 | Invalid amount (positive, at most 2 decimals) |
| E005 | 400 | Duplicate `clientCorrelator` |
| E006 | 401 | Invalid credentials |
| E007 | 403 | Sandbox access not enabled |
| E008 | 404 | Transaction not found |
| E009 | 409 | Refund not eligible (e.g. already refunded) |
| E010 | 422 | Insufficient funds |
| E011 | 422 | Barred MSISDN |
| E012 | 422 | Refund exceeds original amount |
| E013 | 422 | Wallet limit exceeded |
| E014 | 500 | Internal server error (retry with backoff) |
| E015 | 503 | Service unavailable (retry after maintenance) |

The crate does no client-side validation of MSISDNs, currencies or amounts; the API validates them and returns the code.

## Sandbox testing

1. Request sandbox access on the EcoCash developer portal and note your credentials and merchant details.
2. On the portal's **Test Numbers** page, whitelist and OTP-verify your own MSISDN (`263XXXXXXXXX` or `07XXXXXXXX`).
3. Send a charge to that number, then enter one of these PINs on its USSD prompt. The sandbox answers all of them with HTTP 200 and reports the outcome in `statusMessage`:

| PIN | Scenario | Message |
| --- | --- | --- |
| `0000` | Success | Transaction Successful |
| `1111` | Insufficient funds | Insufficient Balance |
| `2222` | Incorrect PIN | Transaction Failed - Invalid PIN |
| `9999` | Limit exceeded | Transaction Limit Exceeded |

### Examples

Credentials are read from the environment, never from files:

| Variable | Purpose |
| --- | --- |
| `ECOCASH_USER`, `ECOCASH_PASS` | Basic Auth credentials (or `ECOCASH_BASIC` = the base64 of `user:pass`) |
| `ECOCASH_PHONE` | Whitelisted test number |
| `ECOCASH_MERCHANT_CODE`, `_PIN`, `_NUMBER`, `_NAME`, `_TERMINAL_ID` | Merchant details (default: shared sandbox merchant) |
| `ECOCASH_CHANNEL`, `ECOCASH_SUPER_MERCHANT`, `ECOCASH_LOCATION` | Optional merchant overrides |
| `ECOCASH_CURRENCY`, `ECOCASH_AMOUNT` | Optional charge overrides (`sandbox` example) |
| `ECOCASH_BASE_URL` | Optional base URL override |
| `ECOCASH_REFUND=1` | Also refund after a successful charge (`sandbox` example) |

```sh
cargo run --example sandbox   # one charge, poll, optional refund
cargo run --example certify   # TC-001..TC-006, writes certification.csv
```

`certify` runs the six test cases from EcoCash's production-access test script (four PIN scenarios, a lookup, and a refund) and writes `certification.csv` with the script's columns (Test Case ID, API Tested, Test PIN Used, Merchant Reference, Expected Result, Actual Result, Status, Comments) so you can paste it into the portal's Excel template. It sends a charge for each PIN scenario and waits while you enter the PIN on the phone.

## Differences from the TypeScript SDK

- Targets the v1 Basic-Auth API instead of the older `X-API-KEY` / `ecocash_pay` endpoints; the legacy client is not included.
- Typed errors with documented codes and a retryability check, a configurable timeout, and no console logging.
- Unique correlators and reference codes are generated for you; polling does not make a redundant initial lookup or sleep after the final attempt.

## Development

```sh
cargo build
cargo test                                   # mock-server tests, no network needed
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The tests use [`wiremock`](https://crates.io/crates/wiremock) to check request paths, headers and bodies, error mapping and polling behaviour.

## Status and roadmap

- [x] Charge, lookup, refund/reversal, polling, typed errors
- [x] Mock-server test suite and sandbox/certification examples
- [ ] **Verified against the live sandbox** (blocked on the pending EcoCash developer issue)
- [ ] Confirm response shapes and the final status after failed PINs
- [ ] Confirm the refund request body and the amount format (number vs string)
- [ ] Production base URL and a first crates.io release

## License

MIT. See [LICENSE](LICENSE).
