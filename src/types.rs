use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Merchant identity sent with every charge and refund.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantConfig {
    pub merchant_code: String,
    pub merchant_pin: String,
    /// Merchant's registered MSISDN.
    pub merchant_number: String,
    pub merchant_name: String,
    pub terminal_id: String,
    pub location: String,
    pub super_merchant_name: String,
    pub country_code: String,
    /// `chargeMetaData.channel`, e.g. `POS`.
    pub channel: String,
}

impl MerchantConfig {
    /// Uses sandbox-friendly defaults: `ZW`, `ECOCASH`, `POS`, `Harare`.
    pub fn new(
        merchant_code: impl Into<String>,
        merchant_pin: impl Into<String>,
        merchant_number: impl Into<String>,
        merchant_name: impl Into<String>,
        terminal_id: impl Into<String>,
    ) -> Self {
        Self {
            merchant_code: merchant_code.into(),
            merchant_pin: merchant_pin.into(),
            merchant_number: merchant_number.into(),
            merchant_name: merchant_name.into(),
            terminal_id: terminal_id.into(),
            location: "Harare".into(),
            super_merchant_name: "ECOCASH".into(),
            country_code: "ZW".into(),
            channel: "POS".into(),
        }
    }
}

impl MerchantConfig {
    /// The shared sandbox merchant (`001535` / `UAT STORE 3`).
    pub fn sandbox() -> Self {
        Self::new("001535", "1234", "788732685", "UAT STORE 3", "UAT00003")
    }
}

/// Outcome derived from `statusMessage` (sandbox PIN matrix: 0000, 1111, 2222, 9999).
/// The sandbox returns HTTP 200 for all of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// "Transaction Successful"
    Successful,
    /// "Insufficient Balance"
    InsufficientBalance,
    /// "Transaction Failed - Invalid PIN"
    InvalidPin,
    /// "Transaction Limit Exceeded"
    LimitExceeded,
    /// Any other message.
    Other,
}

/// `tranType` of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TranType {
    /// `MER` - merchant charge.
    #[default]
    #[serde(rename = "MER")]
    Merchant,
    /// `REF` - refund.
    #[serde(rename = "REF")]
    Refund,
    /// `REV` - reversal.
    #[serde(rename = "REV")]
    Reversal,
}

/// A merchant charge request (API 1).
#[derive(Debug, Clone, Default)]
pub struct ChargeRequest {
    /// Customer MSISDN, e.g. `773047653`.
    pub phone: String,
    pub amount: f64,
    /// Defaults to `USD`.
    pub currency: Option<String>,
    /// Defaults to the merchant name.
    pub description: Option<String>,
    pub remarks: Option<String>,
    pub notify_url: Option<String>,
    /// Generated (UUID) when `None`.
    pub client_correlator: Option<String>,
    /// Generated (`TEST_<correlator>`-style) when `None`.
    pub reference_code: Option<String>,
}

/// A refund or reversal request (API 3).
#[derive(Debug, Clone, Default)]
pub struct RefundRequest {
    /// Customer MSISDN.
    pub phone: String,
    /// Amount to refund; must not exceed the original charge.
    pub amount: f64,
    /// `transactionId` of the original charge.
    pub original_ecocash_reference: String,
    /// `Refund` (default via [`RefundRequest::refund`]) or `Reversal`.
    pub tran_type: TranType,
    pub currency: Option<String>,
    pub description: Option<String>,
    pub remarks: Option<String>,
    pub notify_url: Option<String>,
    pub client_correlator: Option<String>,
    pub reference_code: Option<String>,
}

impl RefundRequest {
    pub fn refund(phone: impl Into<String>, amount: f64, original: impl Into<String>) -> Self {
        Self {
            phone: phone.into(),
            amount,
            original_ecocash_reference: original.into(),
            tran_type: TranType::Refund,
            ..Default::default()
        }
    }

    pub fn reversal(phone: impl Into<String>, amount: f64, original: impl Into<String>) -> Self {
        Self {
            tran_type: TranType::Reversal,
            ..Self::refund(phone, amount, original)
        }
    }
}

/// Response of charge, lookup and refund calls. The API schema is loosely
/// documented, so everything is optional and unknown fields land in `extra`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TransactionResponse {
    pub transaction_id: Option<String>,
    pub client_correlator: Option<String>,
    /// e.g. `PENDING`, `SUCCESS`. Some responses use `transactionStatus` instead;
    /// use [`state`](TransactionResponse::state) to read whichever is present.
    pub status: String,
    /// `PENDING` | `SUCCESS` | `FAILED`, as named in the portal's JS examples.
    pub transaction_status: Option<String>,
    pub status_code: Option<String>,
    pub status_message: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub end_user_id: Option<String>,
    pub merchant_code: Option<String>,
    pub merchant_name: Option<String>,
    pub reference_code: Option<String>,
    /// Refunds: the charge's `transactionId`.
    pub original_reference: Option<String>,
    pub timestamp: Option<String>,
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl TransactionResponse {
    /// `transactionStatus` if present, else `status`.
    pub fn state(&self) -> &str {
        match self.transaction_status.as_deref() {
            Some(t) if !t.is_empty() => t,
            _ => &self.status,
        }
    }

    pub fn is_success(&self) -> bool {
        self.state().eq_ignore_ascii_case("SUCCESS")
    }

    pub fn is_failed(&self) -> bool {
        self.state().eq_ignore_ascii_case("FAILED")
    }

    /// Classifies `statusMessage`. Note an accepted charge also reports
    /// "Transaction Successful" while `status` is still `PENDING`; use
    /// [`is_success`](Self::is_success) for the final payment state.
    pub fn outcome(&self) -> Outcome {
        let m = self
            .status_message
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();
        if m.contains("insufficient") {
            Outcome::InsufficientBalance
        } else if m.contains("invalid pin") {
            Outcome::InvalidPin
        } else if m.contains("limit exceeded") {
            Outcome::LimitExceeded
        } else if m.contains("successful") {
            Outcome::Successful
        } else {
            Outcome::Other
        }
    }

    pub fn is_pending(&self) -> bool {
        self.state().to_ascii_uppercase().starts_with("PENDING")
    }
}

/// How [`Ecocash::poll_transaction`](crate::Ecocash::poll_transaction) waits between lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PollStrategy {
    /// No delay between lookups.
    Simple,
    /// Fixed delay of `sleep` ms between lookups.
    #[default]
    Interval,
    /// Delay starts at `sleep` ms and is multiplied by `multiplier` after each lookup.
    Backoff,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PollOptions {
    pub multiplier: u32,
    /// Initial delay in milliseconds.
    pub sleep: u64,
    /// Maximum number of lookups.
    pub interval: u32,
}

impl Default for PollOptions {
    fn default() -> Self {
        Self {
            multiplier: 2,
            sleep: 1000,
            interval: 10,
        }
    }
}
