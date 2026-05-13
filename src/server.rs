use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use std::sync::Arc;

use crate::dataset::Index;
use crate::types::{FraudRequest, FraudResponse};
use crate::vectorize;
use crate::vptree;

pub struct AppState {
    pub index: Index,
}

pub async fn run(index_path: &str, port: u16) {
    let index = crate::dataset::load_index(index_path);
    let state = Arc::new(AppState { index });

    let app = Router::new()
        .route("/ready", get(ready))
        .route("/fraud-score", post(fraud_score))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ready() -> &'static str {
    "OK"
}

async fn fraud_score(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FraudRequest>,
) -> Json<FraudResponse> {
    let vector = vectorize::vectorize(&req);
    let (fraud_score, approved) = vptree::query_knn(
        &vector,
        &state.index.vectors,
        &state.index.labels,
        &state.index.medians,
        state.index.n,
    );

    Json(FraudResponse {
        approved,
        fraud_score,
    })
}
