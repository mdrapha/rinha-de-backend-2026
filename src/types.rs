use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct FraudRequest {
    pub id: String,
    pub transaction: Transaction,
    pub customer: Customer,
    pub merchant: Merchant,
    pub terminal: Terminal,
    pub last_transaction: Option<LastTransaction>,
}

#[derive(Deserialize)]
pub struct Transaction {
    pub amount: f64,
    pub installments: u32,
    pub requested_at: String,
}

#[derive(Deserialize)]
pub struct Customer {
    pub avg_amount: f64,
    pub tx_count_24h: u32,
    pub known_merchants: Vec<String>,
}

#[derive(Deserialize)]
pub struct Merchant {
    pub id: String,
    pub mcc: String,
    pub avg_amount: f64,
}

#[derive(Deserialize)]
pub struct Terminal {
    pub is_online: bool,
    pub card_present: bool,
    pub km_from_home: f64,
}

#[derive(Deserialize)]
pub struct LastTransaction {
    pub timestamp: String,
    pub km_from_current: f64,
}

#[derive(Serialize)]
pub struct FraudResponse {
    pub approved: bool,
    pub fraud_score: f64,
}
