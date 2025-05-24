use crate::{
    models::{self, MatchedOrder},
    server_state::SharedServerState,
};
use axum::{
    Json,
    extract::{FromRequest, Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use yolo_core::{Order, order_book};

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Bad JSON input: {}", .0.body_text())]
    JsonRejection(#[from] JsonRejection),
    #[error("Order book error: `{0}`")]
    OrderBookError(#[from] order_book::Error),
    #[error("Resource not found")]
    NotFound,
    #[error("Internal server error: `{0}`")]
    Internal(#[from] anyhow::Error),
    #[error("Lock poisoned")]
    PoisonError,
}

#[repr(i64)]
#[derive(Debug)]
enum ServerErrorCode {
    UnknownError = -1,
    BadUserInput = 1,
    OrderBookError = 2,
}

// Add conversion for PoisonError
impl<T> From<std::sync::PoisonError<T>> for ServerError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        ServerError::PoisonError
    }
}

// Tell axum how `ServerError` should be converted into a response.
//
// This is also a convenient place to log errors.
impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        // How we want errors responses to be serialized
        #[derive(Serialize)]
        struct ErrorResponse {
            code: Option<i64>,
            message: String,
        }

        let (status, code) = match self {
            ServerError::JsonRejection(ref rejection) => {
                // This error is caused by bad user input so don't log it
                (rejection.status(), Some(ServerErrorCode::BadUserInput))
            }
            ServerError::OrderBookError(ref err) => {
                // Because `TraceLayer` wraps each request in a span that contains the request
                // method, uri, etc we don't need to include those details here
                tracing::error!(%err, "error from order_book module");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Some(ServerErrorCode::OrderBookError),
                )
            }
            ServerError::NotFound => (StatusCode::NOT_FOUND, None),
            ServerError::PoisonError | ServerError::Internal(_) => {
                tracing::error!(error = %self, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Some(ServerErrorCode::UnknownError),
                )
            }
        };

        (
            status,
            AppJson(ErrorResponse {
                message: self.to_string(),
                code: code.map(|c| c as i64),
            }),
        )
            .into_response()
    }
}

// Create our own JSON extractor by wrapping `axum::Json`. This makes it easy to override the
// rejection and provide our own which formats errors to match our application.
//
// `axum::Json` responds with plain text if the input is invalid.
#[derive(FromRequest)]
#[from_request(via(Json), rejection(ServerError))]
pub struct AppJson<T>(T);

impl<T> IntoResponse for AppJson<T>
where
    Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Bid,
    Ask,
}

impl From<OrderSide> for yolo_core::Side {
    fn from(val: OrderSide) -> Self {
        match val {
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
) -> Result<impl IntoResponse, ServerError> {
    let state = state.read()?;
    if let Some(order_book) = state.exchange.get(&pair) {
        Ok(Json(models::OrderBook::from(order_book)))
    } else {
        Err(ServerError::NotFound)
    }
}

pub async fn create_limit_order(
    State(state): State<SharedServerState>,
    Path(pair): Path<String>,
    Json(payload): Json<CreateLimitOrder>,
) -> Result<impl IntoResponse, ServerError> {
    let mut state = state.write()?;
    let order_book = state.exchange.get_mut(&pair).ok_or(ServerError::NotFound)?;
    let order = Order::new(payload.side.into(), payload.size);
    order_book.place_limit_order(payload.price, &order);
    let response = models::Order::from((&order, payload.price));
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn create_market_order(
    State(state): State<SharedServerState>,
    Path(pair): Path<String>,
    Json(payload): Json<CreateMarketOrder>,
) -> Result<impl IntoResponse, ServerError> {
    let mut state = state.write()?;
    let order_book = state.exchange.get_mut(&pair).ok_or(ServerError::NotFound)?;
    let mut order = Order::new(payload.side.into(), payload.size);
    let order_matches = order_book.place_market_order(&mut order)?;
    let matched_orders: Vec<MatchedOrder> = order_matches
        .iter()
        .map(|order_match| (order_match, &order).into())
        .collect();
    Ok((StatusCode::OK, Json(matched_orders)))
}

pub async fn cancel_order(
    State(state): State<SharedServerState>,
    Path(pair): Path<String>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ServerError> {
    let mut state = state.write()?;
    let order_book = state.exchange.get_mut(&pair).ok_or(ServerError::NotFound)?;
    order_book.cancel_order(id)?;
    Ok(StatusCode::NO_CONTENT)
}
