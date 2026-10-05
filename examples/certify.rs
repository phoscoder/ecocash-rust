//! Runs the six pre-populated certification test cases (TC-001..TC-006) and writes
//! `certification.csv` with the script's columns, ready to paste into the Excel
//! template from the portal's Production tab.
//!
//! Needs ECOCASH_USER + ECOCASH_PASS (or ECOCASH_BASIC) and ECOCASH_PHONE.
//! For TC-001..004 the example sends a charge and asks you to enter the stated PIN
//! on the phone's USSD prompt; it then polls for the result.
#[path = "common/mod.rs"]
mod common;

use std::io::{self, Write};

use ecocash::{
    ChargeRequest, Outcome, PollOptions, PollStrategy, RefundRequest, TransactionResponse,
};

struct Row {
    id: &'static str,
    api: &'static str,
    pin: &'static str,
    reference: String,
    expected: &'static str,
    actual: String,
    pass: bool,
    comments: String,
}

fn csv(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn describe(r: &TransactionResponse) -> String {
    format!(
        "status={} statusCode={} message={}",
        r.status,
        r.status_code.as_deref().unwrap_or("-"),
        r.status_message.as_deref().unwrap_or("-")
    )
}

#[tokio::main]
async fn main() {
    let client = common::client();
    let phone = common::env("ECOCASH_PHONE");
    let amount = 2.0;
    let mut rows: Vec<Row> = Vec::new();
    let mut first_success: Option<TransactionResponse> = None;
    let mut first_charge: Option<TransactionResponse> = None;

    let charge_cases = [
        (
            "TC-001",
            "0000",
            "Transaction Successful",
            Outcome::Successful,
        ),
        (
            "TC-002",
            "1111",
            "Insufficient Balance",
            Outcome::InsufficientBalance,
        ),
        (
            "TC-003",
            "2222",
            "Transaction Failed - Invalid PIN",
            Outcome::InvalidPin,
        ),
        (
            "TC-004",
            "9999",
            "Transaction Limit Exceeded",
            Outcome::LimitExceeded,
        ),
    ];
    for (id, pin, expected, want) in charge_cases {
        println!("\n=== {id}: charging {phone}. Enter PIN {pin} on the USSD prompt. ===");
        let reference = format!("{id}_{}", uuid_suffix());
        let req = ChargeRequest {
            phone: phone.clone(),
            amount,
            reference_code: Some(reference.clone()),
            ..Default::default()
        };
        let row = match client.charge(&req).await {
            Err(e) => Row {
                id,
                api: "Charge Request",
                pin,
                reference,
                expected,
                actual: e.to_string(),
                pass: false,
                comments: "request failed".into(),
            },
            Ok(charge) => {
                let tx = client
                    .poll_transaction(
                        &charge,
                        PollStrategy::Interval,
                        PollOptions {
                            sleep: 3000,
                            interval: 40,
                            ..Default::default()
                        },
                    )
                    .await;
                match tx {
                    Err(e) => Row {
                        id,
                        api: "Charge Request",
                        pin,
                        reference,
                        expected,
                        actual: format!("charge: {} | lookup failed: {e}", describe(&charge)),
                        pass: false,
                        comments: "lookup failed".into(),
                    },
                    Ok(tx) => {
                        let pass = charge.outcome() == want
                            || tx.outcome() == want
                            || (want == Outcome::Successful && tx.is_success());
                        if id == "TC-001" && tx.is_success() {
                            first_success = Some(tx.clone());
                        }
                        if first_charge.is_none() {
                            first_charge = Some(charge.clone());
                        }
                        Row {
                            id,
                            api: "Charge Request",
                            pin,
                            reference,
                            expected,
                            actual: format!(
                                "charge: {} | final: {}",
                                describe(&charge),
                                describe(&tx)
                            ),
                            pass,
                            comments: if tx.is_pending() {
                                "still PENDING after polling".into()
                            } else {
                                String::new()
                            },
                        }
                    }
                }
            }
        };
        println!("{} -> {}", row.id, if row.pass { "Pass" } else { "Fail" });
        rows.push(row);
    }

    // TC-005 lookup
    let lookup = match &first_charge {
        Some(c) => client
            .lookup_transaction(
                c.end_user_id.as_deref().unwrap_or(&phone),
                c.client_correlator.as_deref().unwrap_or(""),
            )
            .await
            .map(|r| (describe(&r), !r.status.is_empty()))
            .unwrap_or_else(|e| (e.to_string(), false)),
        None => ("no charge to look up".into(), false),
    };
    rows.push(Row {
        id: "TC-005",
        api: "Transaction Lookup",
        pin: "N/A",
        reference: first_charge
            .as_ref()
            .and_then(|c| c.reference_code.clone())
            .unwrap_or_default(),
        expected: "Transaction status returned",
        actual: lookup.0,
        pass: lookup.1,
        comments: String::new(),
    });

    // TC-006 refund
    let refund = match &first_success {
        Some(tx) => client
            .refund(&RefundRequest::refund(
                phone.clone(),
                amount,
                tx.transaction_id.clone().unwrap_or_default(),
            ))
            .await
            .map(|r| (describe(&r), r.is_success()))
            .unwrap_or_else(|e| (e.to_string(), false)),
        None => ("skipped: TC-001 did not succeed".into(), false),
    };
    rows.push(Row {
        id: "TC-006",
        api: "Refund / Reversal",
        pin: "N/A",
        reference: String::new(),
        expected: "Refund processed successfully",
        actual: refund.0,
        pass: refund.1,
        comments: String::new(),
    });

    let mut out = String::from("Test Case ID,API Tested,Test PIN Used,Merchant Reference,Expected Result,Actual Result,Status,Comments\n");
    for r in &rows {
        out += &format!(
            "{},{},{},{},{},{},{},{}\n",
            r.id,
            csv(r.api),
            r.pin,
            csv(&r.reference),
            csv(r.expected),
            csv(&r.actual),
            if r.pass { "Pass" } else { "Fail" },
            csv(&r.comments)
        );
    }
    std::fs::write("certification.csv", out).expect("write csv");
    let _ = io::stdout().flush();
    println!(
        "\nWrote certification.csv ({} cases, {} passed)",
        rows.len(),
        rows.iter().filter(|r| r.pass).count()
    );
}

fn uuid_suffix() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default()
}
