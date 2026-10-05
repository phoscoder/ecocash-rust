use serde::Deserialize;
use thiserror::Error;

/// Documented EcoCash error codes (`statusCode` of an error body).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// E001 (400): a required request field is absent or empty.
    MissingRequiredField,
    /// E002 (400): `endUserId` must be a valid Zimbabwe MSISDN, e.g. `773047653`.
    InvalidMsisdn,
    /// E003 (400): currency must be `USD` or `ZWG`.
    InvalidCurrency,
    /// E004 (400): amount must be positive with at most 2 decimal places.
    InvalidAmount,
    /// E005 (400): `clientCorrelator` already used.
    DuplicateCorrelator,
    /// E006 (401): credentials missing, malformed or wrong.
    InvalidCredentials,
    /// E007 (403): sandbox access has not been requested.
    SandboxNotEnabled,
    /// E008 (404): no transaction for that `endUserId` + `clientCorrelator`.
    TransactionNotFound,
    /// E009 (409): transaction state does not allow refund/reversal.
    RefundNotEligible,
    /// E010 (422): customer wallet balance too low (sandbox PIN 1111).
    InsufficientFunds,
    /// E011 (422): customer MSISDN is barred.
    BarredMsisdn,
    /// E012 (422): refund amount exceeds the original amount.
    RefundExceedsOriginal,
    /// E013 (422): wallet transaction limit exceeded (sandbox PIN 9999).
    LimitExceeded,
    /// E014 (500): unexpected server error.
    InternalServerError,
    /// E015 (503): service unavailable / under maintenance.
    ServiceUnavailable,
}

impl ErrorCode {
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code.trim().to_ascii_uppercase().as_str() {
            "E001" => Self::MissingRequiredField,
            "E002" => Self::InvalidMsisdn,
            "E003" => Self::InvalidCurrency,
            "E004" => Self::InvalidAmount,
            "E005" => Self::DuplicateCorrelator,
            "E006" => Self::InvalidCredentials,
            "E007" => Self::SandboxNotEnabled,
            "E008" => Self::TransactionNotFound,
            "E009" => Self::RefundNotEligible,
            "E010" => Self::InsufficientFunds,
            "E011" => Self::BarredMsisdn,
            "E012" => Self::RefundExceedsOriginal,
            "E013" => Self::LimitExceeded,
            "E014" => Self::InternalServerError,
            "E015" => Self::ServiceUnavailable,
            _ => return None,
        })
    }

    /// The `Exxx` string.
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingRequiredField => "E001",
            Self::InvalidMsisdn => "E002",
            Self::InvalidCurrency => "E003",
            Self::InvalidAmount => "E004",
            Self::DuplicateCorrelator => "E005",
            Self::InvalidCredentials => "E006",
            Self::SandboxNotEnabled => "E007",
            Self::TransactionNotFound => "E008",
            Self::RefundNotEligible => "E009",
            Self::InsufficientFunds => "E010",
            Self::BarredMsisdn => "E011",
            Self::RefundExceedsOriginal => "E012",
            Self::LimitExceeded => "E013",
            Self::InternalServerError => "E014",
            Self::ServiceUnavailable => "E015",
        }
    }

    /// Short human-readable title.
    pub fn title(self) -> &'static str {
        match self {
            Self::MissingRequiredField => "Missing required field",
            Self::InvalidMsisdn => "Invalid MSISDN format",
            Self::InvalidCurrency => "Invalid currency",
            Self::InvalidAmount => "Invalid amount",
            Self::DuplicateCorrelator => "Duplicate correlator",
            Self::InvalidCredentials => "Invalid credentials",
            Self::SandboxNotEnabled => "Sandbox not enabled",
            Self::TransactionNotFound => "Transaction not found",
            Self::RefundNotEligible => "Refund not eligible",
            Self::InsufficientFunds => "Insufficient funds",
            Self::BarredMsisdn => "Barred MSISDN",
            Self::RefundExceedsOriginal => "Refund exceeds original",
            Self::LimitExceeded => "Limit exceeded",
            Self::InternalServerError => "Internal server error",
            Self::ServiceUnavailable => "Service unavailable",
        }
    }

    /// What to do about it.
    pub fn hint(self) -> &'static str {
        match self {
            Self::MissingRequiredField => "Fill in every required request field.",
            Self::InvalidMsisdn => "Use a Zimbabwe MSISDN such as 773047653.",
            Self::InvalidCurrency => "Use \"USD\" or \"ZWG\".",
            Self::InvalidAmount => "Use a positive amount with at most 2 decimal places.",
            Self::DuplicateCorrelator => "Use a unique clientCorrelator for every transaction.",
            Self::InvalidCredentials => "Check the Authorization header, username and password.",
            Self::SandboxNotEnabled => "Request Sandbox Access on the Authentication tab first.",
            Self::TransactionNotFound => {
                "Check the endUserId and clientCorrelator of the original request."
            }
            Self::RefundNotEligible => {
                "The transaction cannot be refunded or reversed (e.g. already refunded)."
            }
            Self::InsufficientFunds => "The customer needs to top up their wallet.",
            Self::BarredMsisdn => "This MSISDN cannot transact on EcoCash; use another number.",
            Self::RefundExceedsOriginal => "Refund no more than the original charge amount.",
            Self::LimitExceeded => {
                "The wallet's transaction limit was hit; try a smaller amount later."
            }
            Self::InternalServerError => {
                "Retry with exponential backoff; contact support if it persists."
            }
            Self::ServiceUnavailable => "Retry after the maintenance window.",
        }
    }

    /// Worth retrying with backoff (E014, E015).
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::InternalServerError | Self::ServiceUnavailable)
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.code(), self.title())
    }
}

/// Errors returned by the EcoCash client.
#[derive(Debug, Error)]
pub enum Error {
    /// Network, TLS, timeout or body-decoding failure.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// The API replied with a non-2xx status.
    #[error("api error (http {status}){}: {}", code_suffix(.code), message.as_deref().unwrap_or(body))]
    Api {
        status: u16,
        /// Parsed from `statusCode` when it is a documented `Exxx` code.
        code: Option<ErrorCode>,
        /// `statusMessage` from the error body.
        message: Option<String>,
        /// Raw response body.
        body: String,
    },
    /// The API replied with a body that could not be parsed.
    #[error("invalid response: {0}")]
    Json(#[from] serde_json::Error),
    /// A response passed to a follow-up call lacks a required field.
    #[error("missing field: {0}")]
    MissingField(&'static str),
}

fn code_suffix(code: &Option<ErrorCode>) -> String {
    code.map(|c| format!(" [{c}]")).unwrap_or_default()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    status_code: Option<serde_json::Value>,
    status_message: Option<String>,
}

impl Error {
    pub(crate) fn from_response(status: u16, body: String) -> Self {
        let parsed: Option<ErrorBody> = serde_json::from_str(&body).ok();
        let code = parsed
            .as_ref()
            .and_then(|b| b.status_code.as_ref())
            .and_then(|v| v.as_str())
            .and_then(ErrorCode::from_code);
        let message = parsed.and_then(|b| b.status_message);
        Error::Api {
            status,
            code,
            message,
            body,
        }
    }

    /// Documented error code, if this is an API error that carried one.
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Error::Api { code, .. } => *code,
            _ => None,
        }
    }

    /// Whether retrying with backoff makes sense: transport failures, 5xx, E014, E015.
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Http(_) => true,
            Error::Api { status, code, .. } => {
                *status >= 500 || code.is_some_and(ErrorCode::is_retryable)
            }
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
