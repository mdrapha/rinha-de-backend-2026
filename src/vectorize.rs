use crate::types::FraudRequest;

const MAX_AMOUNT: f64 = 10000.0;
const MAX_INSTALLMENTS: f64 = 12.0;
const AMOUNT_VS_AVG_RATIO: f64 = 10.0;
const MAX_MINUTES: f64 = 1440.0;
const MAX_KM: f64 = 1000.0;
const MAX_TX_COUNT_24H: f64 = 20.0;
const MAX_MERCHANT_AVG_AMOUNT: f64 = 10000.0;

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn mcc_risk(mcc: &str) -> f64 {
    match mcc {
        "5411" => 0.15,
        "5812" => 0.30,
        "5912" => 0.20,
        "5944" => 0.45,
        "7801" => 0.80,
        "7802" => 0.75,
        "7995" => 0.85,
        "4511" => 0.35,
        "5311" => 0.25,
        "5999" => 0.50,
        _ => 0.50,
    }
}

fn parse_hour(ts: &str) -> u32 {
    let b = ts.as_bytes();
    (b[11] - b'0') as u32 * 10 + (b[12] - b'0') as u32
}

fn parse_day_of_week(ts: &str) -> u32 {
    let b = ts.as_bytes();
    let y = parse_num_u32(b, 0, 4) as i32;
    let m = parse_num_u32(b, 5, 2);
    let d = parse_num_u32(b, 8, 2);
    sakamoto_dow(y, m, d)
}

fn sakamoto_dow(mut y: i32, m: u32, d: u32) -> u32 {
    static T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let dow = ((y + y / 4 - y / 100 + y / 400 + T[m as usize - 1] + d as i32) % 7) as u32;
    (dow + 6) % 7
}

fn timestamp_to_seconds(ts: &str) -> i64 {
    let b = ts.as_bytes();
    let y = parse_num_u32(b, 0, 4) as i64;
    let m = parse_num_u32(b, 5, 2) as i64;
    let d = parse_num_u32(b, 8, 2) as i64;
    let h = parse_num_u32(b, 11, 2) as i64;
    let min = parse_num_u32(b, 14, 2) as i64;
    let s = parse_num_u32(b, 17, 2) as i64;

    let days = days_from_epoch(y, m, d);
    days * 86400 + h * 3600 + min * 60 + s
}

fn days_from_epoch(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

fn parse_num_u32(b: &[u8], start: usize, len: usize) -> u32 {
    let mut n = 0u32;
    for i in 0..len {
        n = n * 10 + (b[start + i] - b'0') as u32;
    }
    n
}

pub fn vectorize(req: &FraudRequest) -> [f32; 14] {
    let amount = clamp01(req.transaction.amount / MAX_AMOUNT);
    let installments = clamp01(req.transaction.installments as f64 / MAX_INSTALLMENTS);
    let amount_vs_avg =
        clamp01((req.transaction.amount / req.customer.avg_amount) / AMOUNT_VS_AVG_RATIO);
    let hour_of_day = parse_hour(&req.transaction.requested_at) as f64 / 23.0;
    let day_of_week = parse_day_of_week(&req.transaction.requested_at) as f64 / 6.0;

    let (minutes_since_last_tx, km_from_last_tx) = match &req.last_transaction {
        Some(lt) => {
            let sec_current = timestamp_to_seconds(&req.transaction.requested_at);
            let sec_last = timestamp_to_seconds(&lt.timestamp);
            let minutes = (sec_current - sec_last).abs() as f64 / 60.0;
            (
                clamp01(minutes / MAX_MINUTES),
                clamp01(lt.km_from_current / MAX_KM),
            )
        }
        None => (-1.0, -1.0),
    };

    let km_from_home = clamp01(req.terminal.km_from_home / MAX_KM);
    let tx_count_24h = clamp01(req.customer.tx_count_24h as f64 / MAX_TX_COUNT_24H);
    let is_online = if req.terminal.is_online { 1.0 } else { 0.0 };
    let card_present = if req.terminal.card_present { 1.0 } else { 0.0 };
    let unknown_merchant = if req.customer.known_merchants.contains(&req.merchant.id) {
        0.0
    } else {
        1.0
    };
    let mcc = mcc_risk(&req.merchant.mcc);
    let merchant_avg_amount = clamp01(req.merchant.avg_amount / MAX_MERCHANT_AVG_AMOUNT);

    [
        amount as f32,
        installments as f32,
        amount_vs_avg as f32,
        hour_of_day as f32,
        day_of_week as f32,
        minutes_since_last_tx as f32,
        km_from_last_tx as f32,
        km_from_home as f32,
        tx_count_24h as f32,
        is_online as f32,
        card_present as f32,
        unknown_merchant as f32,
        mcc as f32,
        merchant_avg_amount as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn vectorize_legit_example() {
        let req = FraudRequest {
            id: "tx-1329056812".into(),
            transaction: Transaction {
                amount: 41.12,
                installments: 2,
                requested_at: "2026-03-11T18:45:53Z".into(),
            },
            customer: Customer {
                avg_amount: 82.24,
                tx_count_24h: 3,
                known_merchants: vec!["MERC-003".into(), "MERC-016".into()],
            },
            merchant: Merchant {
                id: "MERC-016".into(),
                mcc: "5411".into(),
                avg_amount: 60.25,
            },
            terminal: Terminal {
                is_online: false,
                card_present: true,
                km_from_home: 29.23,
            },
            last_transaction: None,
        };

        let v = vectorize(&req);
        let expected: [f32; 14] = [
            0.004112, 0.1667, 0.05, 0.7826, 0.3333, -1.0, -1.0, 0.02923, 0.15, 0.0, 1.0, 0.0,
            0.15, 0.006025,
        ];

        for i in 0..14 {
            assert!(
                approx_eq(v[i], expected[i], 0.001),
                "dim {}: got {} expected {}",
                i,
                v[i],
                expected[i]
            );
        }
    }

    #[test]
    fn vectorize_fraud_example() {
        let req = FraudRequest {
            id: "tx-3330991687".into(),
            transaction: Transaction {
                amount: 9505.97,
                installments: 10,
                requested_at: "2026-03-14T05:15:12Z".into(),
            },
            customer: Customer {
                avg_amount: 81.28,
                tx_count_24h: 20,
                known_merchants: vec![
                    "MERC-008".into(),
                    "MERC-007".into(),
                    "MERC-005".into(),
                ],
            },
            merchant: Merchant {
                id: "MERC-068".into(),
                mcc: "7802".into(),
                avg_amount: 54.86,
            },
            terminal: Terminal {
                is_online: false,
                card_present: true,
                km_from_home: 952.27,
            },
            last_transaction: None,
        };

        let v = vectorize(&req);
        let expected: [f32; 14] = [
            0.9506, 0.8333, 1.0, 0.2174, 0.8333, -1.0, -1.0, 0.9523, 1.0, 0.0, 1.0, 1.0, 0.75,
            0.005486,
        ];

        for i in 0..14 {
            assert!(
                approx_eq(v[i], expected[i], 0.001),
                "dim {}: got {} expected {}",
                i,
                v[i],
                expected[i]
            );
        }
    }

    #[test]
    fn vectorize_with_last_transaction() {
        let req = FraudRequest {
            id: "tx-100".into(),
            transaction: Transaction {
                amount: 384.88,
                installments: 3,
                requested_at: "2026-03-11T20:23:35Z".into(),
            },
            customer: Customer {
                avg_amount: 769.76,
                tx_count_24h: 3,
                known_merchants: vec![
                    "MERC-009".into(),
                    "MERC-001".into(),
                    "MERC-001".into(),
                ],
            },
            merchant: Merchant {
                id: "MERC-001".into(),
                mcc: "5912".into(),
                avg_amount: 298.95,
            },
            terminal: Terminal {
                is_online: false,
                card_present: true,
                km_from_home: 13.709,
            },
            last_transaction: Some(LastTransaction {
                timestamp: "2026-03-11T14:58:35Z".into(),
                km_from_current: 18.8626,
            }),
        };

        let v = vectorize(&req);

        assert!(v[5] >= 0.0 && v[5] <= 1.0, "minutes should be normalized");
        assert!(v[6] >= 0.0 && v[6] <= 1.0, "km should be normalized");

        let expected_minutes = 325.0 / 1440.0;
        assert!(
            approx_eq(v[5], expected_minutes as f32, 0.001),
            "minutes: got {} expected {}",
            v[5],
            expected_minutes
        );
    }

    #[test]
    fn day_of_week_known_dates() {
        assert_eq!(sakamoto_dow(2026, 3, 11), 2); // Wednesday
        assert_eq!(sakamoto_dow(2026, 3, 14), 5); // Saturday
        assert_eq!(sakamoto_dow(2026, 3, 9), 0); // Monday
        assert_eq!(sakamoto_dow(2026, 3, 15), 6); // Sunday
    }
}
