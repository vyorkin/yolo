use crate::{models, server_state::SharedServerState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use yolo_core::Order;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Bid,
    Ask,
}

impl Into<yolo_core::Side> for OrderSide {
    fn into(self) -> yolo_core::Side {
        match self {
            OrderSide::Bid => yolo_core::Side::Bid,
            OrderSide::Ask => yolo_core::Side::Ask,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateLimitOrder {
    pub side: OrderSide,
    pub size: Decimal,
    pub price: Decimal,
}

#[derive(Deserialize)]
pub struct CreateMarketOrder {
    pub side: OrderSide,
    pub size: Decimal,
}

pub async fn order_book_index(
    Path(pair): Path<String>,
    State(state): State<SharedServerState>,
) -> impl IntoResponse {
    let state = state.read().unwrap();
    let order_book = state.exchange.get(&pair).unwrap();
    Json(models::OrderBook::from(order_book))
}

pub async fn create_limit_order(
    State(state): State<SharedServerState>,
    Path(pair): Path<String>,
    Json(payload): Json<CreateLimitOrder>,
) -> impl IntoResponse {
    let mut state = state.write().unwrap();
    let order_book = state.exchange.get_mut(&pair).unwrap();

    let order = Order::new(payload.side.into(), payload.size);
    order_book.place_limit_order(payload.price, &order);

    let response = models::Order::from((&order, payload.price));

    (StatusCode::CREATED, Json(response))
}

// pub async fn create_market_order() -> impl IntoResponse {
//     todo!();
// }
//
// pub async fn cancel_order() -> impl IntoResponse {
//     todo!();
// }
