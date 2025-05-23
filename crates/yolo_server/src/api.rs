use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{models, server_state::SharedServerState};

#[derive(Deserialize)]
pub enum OrderSide {
    Bid,
    Ask,
}

#[derive(Deserialize)]
pub struct CreateLimitOrder {
    pub side: OrderSide,
    pub size: String,
    pub price: String,
}

#[derive(Deserialize)]
pub struct CreateMarketOrder {
    pub side: OrderSide,
    pub size: String,
}

pub async fn order_book_index(
    Path(pair): Path<String>,
    State(state): State<SharedServerState>,
) -> impl IntoResponse {
    let state = state.read().unwrap();
    let order_book = state.exchange.get(&pair).unwrap();
    let order_book_response = models::OrderBook::from(order_book);

    Json(order_book_response)
}
