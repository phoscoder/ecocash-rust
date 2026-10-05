// Shared helpers for the examples (included with `#[path]`).
use ecocash::{Ecocash, MerchantConfig};

pub fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("set {k}"))
}

pub fn opt(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

/// Credentials and merchant from the environment. Merchant fields default to the
/// shared sandbox merchant; override with ECOCASH_MERCHANT_CODE / _PIN / _NUMBER /
/// _NAME / _TERMINAL_ID / _CHANNEL / _SUPER_MERCHANT / _LOCATION. ECOCASH_BASE_URL overrides the API base URL.
pub fn client() -> Ecocash {
    let d = MerchantConfig::sandbox();
    let mut merchant = MerchantConfig::new(
        opt("ECOCASH_MERCHANT_CODE").unwrap_or(d.merchant_code),
        opt("ECOCASH_MERCHANT_PIN").unwrap_or(d.merchant_pin),
        opt("ECOCASH_MERCHANT_NUMBER").unwrap_or(d.merchant_number),
        opt("ECOCASH_MERCHANT_NAME").unwrap_or(d.merchant_name),
        opt("ECOCASH_TERMINAL_ID").unwrap_or(d.terminal_id),
    );
    if let Some(v) = opt("ECOCASH_CHANNEL") {
        merchant.channel = v;
    }
    if let Some(v) = opt("ECOCASH_SUPER_MERCHANT") {
        merchant.super_merchant_name = v;
    }
    if let Some(v) = opt("ECOCASH_LOCATION") {
        merchant.location = v;
    }
    let mut b = match opt("ECOCASH_BASIC") {
        Some(enc) => Ecocash::builder_encoded(&enc, merchant),
        None => Ecocash::builder(&env("ECOCASH_USER"), &env("ECOCASH_PASS"), merchant),
    };
    if let Some(url) = opt("ECOCASH_BASE_URL") {
        b = b.base_url(url);
    }
    b.build().expect("client")
}
