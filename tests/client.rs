use std::time::Duration;

use ecocash::{
    ChargeRequest, Ecocash, Error, ErrorCode, MerchantConfig, PollOptions, PollStrategy,
    RefundRequest,
};
use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// base64("user:pass")
const AUTH: &str = "Basic dXNlcjpwYXNz";

fn client(server: &MockServer) -> Ecocash {
    let m = MerchantConfig::new("001535", "1234", "788732685", "UAT STORE 3", "UAT00003");
    Ecocash::builder("user", "pass", m)
        .base_url(server.uri())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn charge_posts_documented_body_with_basic_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/transactions/amount/"))
        .and(header("Authorization", AUTH))
        .and(body_partial_json(json!({
            "clientCorrelator": "c1",
            "tranType": "MER",
            "endUserId": "773047653",
            "paymentAmount": {
                "charginginformation": {"amount": 2.0, "currency": "USD", "description": "UAT STORE 3"},
                "chargeMetaData": {"channel": "POS"}
            },
            "merchantCode": "001535", "merchantPin": "1234", "merchantNumber": "788732685",
            "countryCode": "ZW", "terminalID": "UAT00003", "location": "Harare",
            "superMerchantName": "ECOCASH", "merchantName": "UAT STORE 3"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transactionId": "MP1", "status": "PENDING", "statusCode": "200", "amount": 2, "currency": "USD"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let r = client(&server)
        .charge(&ChargeRequest {
            phone: "773047653".into(),
            amount: 2.0,
            client_correlator: Some("c1".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(r.transaction_id.as_deref(), Some("MP1"));
    assert!(r.is_pending() && !r.is_success());
    assert_eq!(r.client_correlator.as_deref(), Some("c1"));
    assert_eq!(r.end_user_id.as_deref(), Some("773047653"));
}

#[tokio::test]
async fn lookup_is_get_with_path_params() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/773047653/transactions/amount/c1"))
        .and(header("Authorization", AUTH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "SUCCESS", "amount": 2, "foo": "bar"})),
        )
        .mount(&server)
        .await;
    let r = client(&server)
        .lookup_transaction("773047653", "c1")
        .await
        .unwrap();
    assert!(r.is_success());
    assert_eq!(r.extra["foo"], "bar");
}

#[tokio::test]
async fn refund_and_reversal_set_tran_type_and_original_reference() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/transactions/refund/"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "SUCCESS", "originalReference": "MP1"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    let c = client(&server);
    let r = c
        .refund(&RefundRequest::refund("773047653", 2.0, "MP1"))
        .await
        .unwrap();
    assert!(r.is_success() && r.client_correlator.is_some());
    c.refund(&RefundRequest::reversal("773047653", 2.0, "MP1"))
        .await
        .unwrap();

    let reqs = server.received_requests().await.unwrap();
    let b0: Value = serde_json::from_slice(&reqs[0].body).unwrap();
    let b1: Value = serde_json::from_slice(&reqs[1].body).unwrap();
    assert_eq!(b0["tranType"], "REF");
    assert_eq!(b1["tranType"], "REV");
    assert_eq!(b0["originalEcocashReference"], "MP1");
}

#[tokio::test]
async fn non_2xx_maps_to_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_string("nf"))
        .mount(&server)
        .await;
    match client(&server).lookup_transaction("1", "2").await {
        Err(Error::Api {
            status: 404,
            body,
            code: None,
            ..
        }) => assert_eq!(body, "nf"),
        other => panic!("unexpected: {other:?}"),
    }
}

fn charge_resp() -> ecocash::TransactionResponse {
    ecocash::TransactionResponse {
        end_user_id: Some("1".into()),
        client_correlator: Some("c".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn poll_returns_last_pending_after_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "PENDING"})))
        .expect(3)
        .mount(&server)
        .await;
    let r = client(&server)
        .poll_transaction(
            &charge_resp(),
            PollStrategy::Backoff,
            PollOptions {
                multiplier: 2,
                sleep: 5,
                interval: 3,
            },
        )
        .await
        .unwrap();
    assert!(r.is_pending());
}

#[tokio::test]
async fn poll_stops_on_terminal_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "FAILED"})))
        .expect(1)
        .mount(&server)
        .await;
    let r = client(&server)
        .poll_transaction(&charge_resp(), PollStrategy::Simple, PollOptions::default())
        .await
        .unwrap();
    assert!(!r.is_success() && !r.is_pending());
}

#[tokio::test]
async fn poll_requires_correlator() {
    let server = MockServer::start().await;
    let err = client(&server)
        .poll_transaction(
            &Default::default(),
            PollStrategy::Simple,
            PollOptions::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, Error::MissingField(_)));
}

#[tokio::test]
async fn documented_error_codes_are_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "statusCode": "E010", "statusMessage": "Customer wallet balance is too low"
        })))
        .mount(&server)
        .await;
    let err = client(&server)
        .charge(&ChargeRequest {
            phone: "773047653".into(),
            amount: 2.0,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), Some(ErrorCode::InsufficientFunds));
    assert!(!err.is_retryable());
    assert!(err.to_string().contains("E010"));
    match err {
        Error::Api {
            status: 422,
            message,
            ..
        } => assert_eq!(
            message.as_deref(),
            Some("Customer wallet balance is too low")
        ),
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn service_unavailable_is_retryable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(json!({"statusCode": "E015", "statusMessage": "maintenance"})),
        )
        .mount(&server)
        .await;
    let err = client(&server)
        .lookup_transaction("1", "2")
        .await
        .unwrap_err();
    assert_eq!(err.code(), Some(ErrorCode::ServiceUnavailable));
    assert!(err.is_retryable());
}

#[test]
fn outcome_classifies_status_message() {
    use ecocash::{Outcome, TransactionResponse};
    let o = |m: &str| {
        TransactionResponse {
            status_message: Some(m.into()),
            ..Default::default()
        }
        .outcome()
    };
    assert_eq!(o("Transaction Successful"), Outcome::Successful);
    assert_eq!(o("Insufficient Balance"), Outcome::InsufficientBalance);
    assert_eq!(o("Transaction Failed - Invalid PIN"), Outcome::InvalidPin);
    assert_eq!(o("Transaction Limit Exceeded"), Outcome::LimitExceeded);
    assert_eq!(o("???"), Outcome::Other);
}

#[test]
fn transaction_status_field_is_understood() {
    let r: ecocash::TransactionResponse =
        serde_json::from_value(json!({"transactionStatus": "SUCCESS"})).unwrap();
    assert!(r.is_success());
    let r: ecocash::TransactionResponse =
        serde_json::from_value(json!({"transactionStatus": "FAILED"})).unwrap();
    assert!(r.is_failed() && !r.is_pending());
}
