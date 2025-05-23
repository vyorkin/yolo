use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use rust_decimal::dec;
use yolo_core::{Order, OrderBook};

type Exchange = HashMap<String, OrderBook>;

pub struct ServerState {
    pub exchange: Exchange,
}

impl Default for ServerState {
    fn default() -> Self {
        let mut exchange = Exchange::new();
        let mut order_book = OrderBook::new();
        order_book.place_limit_order(dec!(100.0), &Order::ask(dec!(10.0)));
        exchange.insert("usdt_eth".to_string(), order_book);
        Self { exchange }
    }
}

pub type SharedServerState = Arc<RwLock<ServerState>>;
