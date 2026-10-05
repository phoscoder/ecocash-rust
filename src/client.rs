use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::types::*;

/// Sandbox base URL. Pass the production URL via [`EcocashBuilder::base_url`].
pub const SANDBOX_BASE_URL: &str = "https://developers.ecocash.co.zw/sandbox/payment";

/// EcoCash payment API client (Basic Auth).
#[derive(Debug, Clone)]
pub struct Ecocash {
    auth_header: String,
    merchant: MerchantConfig,
    base_url: String,
    http: reqwest::Client,
}

/// Builder for [`Ecocash`].
#[derive(Debug, Clone)]
pub struct EcocashBuilder {
    auth_header: String,
    merchant: MerchantConfig,
    base_url: String,
    timeout: Duration,
}

impl EcocashBuilder {
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> Result<Ecocash> {
        Ok(Ecocash {
            auth_header: self.auth_header,
            merchant: self.merchant,
            base_url: self.base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder().timeout(self.timeout).build()?,
        })
    }
}

impl Ecocash {
    /// Client from a username and password, sent as HTTP Basic Auth.
    pub fn builder(username: &str, password: &str, merchant: MerchantConfig) -> EcocashBuilder {
        let encoded = STANDARD.encode(format!("{username}:{password}"));
        Self::builder_encoded(&encoded, merchant)
    }

    /// Client from the already base64-encoded `<user>:<password>` credentials.
    pub fn builder_encoded(base64_credentials: &str, merchant: MerchantConfig) -> EcocashBuilder {
        EcocashBuilder {
            auth_header: format!("Basic {base64_credentials}"),
            merchant,
            base_url: SANDBOX_BASE_URL.to_string(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Charge `request.phone`; the customer gets a USSD PIN prompt.
    ///
    /// The returned response always carries `client_correlator` and `end_user_id`
    /// so it can be passed to [`poll_transaction`](Self::poll_transaction).
    pub async fn charge(&self, request: &ChargeRequest) -> Result<TransactionResponse> {
        let correlator = request
            .client_correlator
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        let body = self.payload(Payload {
            tran_type: TranType::Merchant,
            phone: &request.phone,
            amount: request.amount,
            currency: request.currency.as_deref(),
            description: request.description.as_deref(),
            remarks: request.remarks.as_deref(),
            notify_url: request.notify_url.as_deref(),
            correlator: &correlator,
            reference_code: request.reference_code.as_deref(),
            original: None,
        });
        let mut resp = self.post("v1/transactions/amount/", &body).await?;
        resp.client_correlator.get_or_insert(correlator);
        resp.end_user_id
            .get_or_insert_with(|| request.phone.clone());
        Ok(resp)
    }

    /// Fetch a transaction by customer MSISDN and the correlator used to create it.
    pub async fn lookup_transaction(
        &self,
        end_user_id: &str,
        client_correlator: &str,
    ) -> Result<TransactionResponse> {
        let url = format!(
            "{}/v1/{end_user_id}/transactions/amount/{client_correlator}",
            self.base_url
        );
        let req = self
            .http
            .get(url)
            .header("Authorization", &self.auth_header);
        Self::send(req).await
    }

    /// Refund or reverse a completed charge.
    pub async fn refund(&self, request: &RefundRequest) -> Result<TransactionResponse> {
        let correlator = request
            .client_correlator
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        let body = self.payload(Payload {
            tran_type: request.tran_type,
            phone: &request.phone,
            amount: request.amount,
            currency: request.currency.as_deref(),
            description: request.description.as_deref(),
            remarks: request.remarks.as_deref(),
            notify_url: request.notify_url.as_deref(),
            correlator: &correlator,
            reference_code: request.reference_code.as_deref(),
            original: Some(&request.original_ecocash_reference),
        });
        let mut resp = self.post("v1/transactions/refund/", &body).await?;
        resp.client_correlator.get_or_insert(correlator);
        Ok(resp)
    }

    /// Look the charge up until it leaves `PENDING*` or `options.interval` lookups
    /// are used up; the last response is returned either way, so check
    /// [`TransactionResponse::is_success`].
    pub async fn poll_transaction(
        &self,
        charge: &TransactionResponse,
        strategy: PollStrategy,
        options: PollOptions,
    ) -> Result<TransactionResponse> {
        let (Some(phone), Some(correlator)) = (&charge.end_user_id, &charge.client_correlator)
        else {
            return Err(Error::MissingField("end_user_id / client_correlator"));
        };
        let defaults = PollOptions::default();
        let attempts = if options.interval == 0 {
            defaults.interval
        } else {
            options.interval
        };
        let multiplier = if options.multiplier == 0 {
            defaults.multiplier
        } else {
            options.multiplier
        };
        let mut sleep = if options.sleep == 0 {
            defaults.sleep
        } else {
            options.sleep
        };

        let mut last = TransactionResponse::default();
        for attempt in 0..attempts {
            last = self.lookup_transaction(phone, correlator).await?;
            if !last.is_pending() {
                return Ok(last);
            }
            if attempt + 1 == attempts || strategy == PollStrategy::Simple {
                continue;
            }
            tokio::time::sleep(Duration::from_millis(sleep)).await;
            if strategy == PollStrategy::Backoff {
                sleep = sleep.saturating_mul(multiplier as u64);
            }
        }
        Ok(last)
    }

    fn payload(&self, p: Payload<'_>) -> Value {
        let m = &self.merchant;
        let reference = p
            .reference_code
            .map(str::to_owned)
            .unwrap_or_else(|| format!("REF_{}", p.correlator));
        let mut body = json!({
            "clientCorrelator": p.correlator,
            "notifyUrl": p.notify_url.unwrap_or(""),
            "referenceCode": reference,
            "tranType": p.tran_type,
            "endUserId": p.phone,
            "remarks": p.remarks.unwrap_or("EcoCash"),
            "transactionOperationStatus": "Charged",
            "paymentAmount": {
                "charginginformation": {
                    "amount": p.amount,
                    "currency": p.currency.unwrap_or("USD"),
                    "description": p.description.unwrap_or(&m.merchant_name),
                },
                "chargeMetaData": { "channel": m.channel },
            },
            "merchantCode": m.merchant_code,
            "merchantPin": m.merchant_pin,
            "merchantNumber": m.merchant_number,
            "countryCode": m.country_code,
            "terminalID": m.terminal_id,
            "location": m.location,
            "superMerchantName": m.super_merchant_name,
            "merchantName": m.merchant_name,
        });
        if let Some(original) = p.original {
            body["originalEcocashReference"] = json!(original);
        }
        body
    }

    async fn post(&self, path: &str, body: &Value) -> Result<TransactionResponse> {
        let req = self
            .http
            .post(format!("{}/{path}", self.base_url))
            .header("Authorization", &self.auth_header)
            .json(body);
        Self::send(req).await
    }

    async fn send(req: reqwest::RequestBuilder) -> Result<TransactionResponse> {
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::from_response(status.as_u16(), text));
        }
        Ok(serde_json::from_str(&text)?)
    }
}

struct Payload<'a> {
    tran_type: TranType,
    phone: &'a str,
    amount: f64,
    currency: Option<&'a str>,
    description: Option<&'a str>,
    remarks: Option<&'a str>,
    notify_url: Option<&'a str>,
    correlator: &'a str,
    reference_code: Option<&'a str>,
    original: Option<&'a str>,
}
