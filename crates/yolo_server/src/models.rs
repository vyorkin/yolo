use rust_decimal::Decimal;
use serde::Serialize;
use std::cmp::Reverse;
use uuid::Uuid;

#[derive(Serialize)]
pub struct Order {
    pub id: Uuid,
    pub price: Decimal,
    pub size: Decimal,
    pub timestamp: i64,
}

impl From<(&yolo_core::Order, Decimal)> for Order {
    fn from((order, price): (&yolo_core::Order, Decimal)) -> Self {
        Order {
            id: order.id,
            price,
            size: order.size,
            timestamp: order.timestamp,
        }
    }
}

#[derive(Serialize)]
pub struct MatchedOrder {
    pub id: Uuid,
    pub price: Decimal,
    pub size: Decimal,
}

impl From<(&yolo_core::OrderMatch, &yolo_core::Order)> for MatchedOrder {
    fn from((order_match, order): (&yolo_core::OrderMatch, &yolo_core::Order)) -> Self {
        let id = if order.side == yolo_core::Side::Bid {
            order_match.ask.id
        } else {
            order_match.bid.id
        };

        MatchedOrder {
            id,
            price: order_match.price,
            size: order_match.size_filled,
        }
    }
}

#[derive(Serialize)]
pub struct OrderBook {
    asks: Vec<Order>,
    bids: Vec<Order>,
    ask_total_volume: Decimal,
    bid_total_volume: Decimal,
}

impl From<&yolo_core::OrderBook> for OrderBook {
    fn from(order_book: &yolo_core::OrderBook) -> Self {
        let asks = order_book
            .asks
            .iter()
            .flat_map(|(&price, limit)| {
                limit
                    .orders_by_uuid
                    .values()
                    .map(move |order| Order::from((order, price)))
            })
            .collect();

        let bids = order_book
            .bids
            .iter()
            .flat_map(|(&Reverse(price), limit)| {
                limit
                    .orders_by_uuid
                    .values()
                    .map(move |order| Order::from((order, price)))
            })
            .collect();

        OrderBook {
            asks,
            bids,
            bid_total_volume: order_book.bid_total_volume,
            ask_total_volume: order_book.ask_total_volume,
        }
    }
}
