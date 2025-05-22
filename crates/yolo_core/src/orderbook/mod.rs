mod limit;
mod order;

pub use limit::*;
pub use order::*;

use rust_decimal::{Decimal, dec};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum OrderBookError {
    #[error("inconsistent order book state")]
    InconsistentState,
    #[error("limit at price `{0}` not found")]
    LimitNotFound(Decimal),
    #[error("order `{0}` not found")]
    OrderNotFound(Uuid),
    #[error(
        "not enough total volume in {} = {actual_volume}, expected at least {expected_volume}", .side.opposite()
    )]
    NotEnoughVolume {
        side: Side,
        expected_volume: Decimal,
        actual_volume: Decimal,
    },
}

#[derive(Debug)]
pub struct Match {
    ask: Order,
    bid: Order,
    size_filled: Decimal,
    price: Decimal,
}

pub struct OrderBook {
    asks: BTreeMap<Decimal, Limit>,
    bids: BTreeMap<Reverse<Decimal>, Limit>,
    ask_total_volume: Decimal,
    bid_total_volume: Decimal,
    order_index: HashMap<Uuid, (Side, Decimal)>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
            ask_total_volume: dec!(0),
            bid_total_volume: dec!(0),
            order_index: HashMap::new(),
        }
    }

    fn ensure_volume(&self, order: &Order) -> Result<(), OrderBookError> {
        let total_volume = match order.side {
            Side::Bid => self.ask_total_volume,
            Side::Ask => self.bid_total_volume,
        };

        if order.size > total_volume {
            Err(OrderBookError::NotEnoughVolume {
                side: order.side,
                expected_volume: order.size,
                actual_volume: total_volume,
            })
        } else {
            Ok(())
        }
    }

    pub fn cancel_order(&mut self, id: Uuid) -> Result<Order, OrderBookError> {
        let (side, price) = self
            .order_index
            .remove(&id)
            .ok_or(OrderBookError::OrderNotFound(id))?;

        let cancelled_oreder = match side {
            Side::Bid => self.cancel_bid_order(id, price),
            Side::Ask => self.cancel_ask_order(id, price),
        };

        cancelled_oreder.ok_or(OrderBookError::OrderNotFound(id))
    }

    fn cancel_bid_order(&mut self, id: Uuid, price: Decimal) -> Option<Order> {
        let key = Reverse(price);
        let limit = self.bids.get_mut(&key)?;
        let removed_order = limit.remove_order(id)?;
        self.bid_total_volume -= removed_order.size;
        if limit.is_empty() {
            self.bids.remove(&key)?;
        }
        Some(removed_order)
    }

    fn cancel_ask_order(&mut self, id: Uuid, price: Decimal) -> Option<Order> {
        let key = price;
        let limit = self.asks.get_mut(&key)?;
        let removed_order = limit.remove_order(id)?;
        self.ask_total_volume -= removed_order.size;
        if limit.is_empty() {
            self.asks.remove(&key)?;
        }
        Some(removed_order)
    }

    pub fn place_market_order(&mut self, order: &mut Order) -> Result<Vec<Match>, OrderBookError> {
        self.ensure_volume(order)?;

        match order.side {
            Side::Bid => self.place_market_bid_order(order),
            Side::Ask => self.place_market_ask_order(order),
        }
    }

    fn place_market_bid_order(&mut self, order: &mut Order) -> Result<Vec<Match>, OrderBookError> {
        let mut matches = Vec::new();
        let mut empty_price_leves = Vec::new();

        // For bid market order, match against asks (in asc order)
        for (&price, limit) in &mut self.asks {
            if order.is_filled() {
                break;
            }

            let mut limit_matches = limit.fill(order);
            let sized_filled: Decimal = limit_matches.iter().map(|m| m.size_filled).sum();
            self.ask_total_volume -= sized_filled;
            matches.append(&mut limit_matches);

            if limit.is_empty() {
                empty_price_leves.push(price);
            }
        }

        for price in empty_price_leves {
            self.asks.remove(&price);
        }

        Ok(matches)
    }

    fn place_market_ask_order(&mut self, order: &mut Order) -> Result<Vec<Match>, OrderBookError> {
        let mut matches = Vec::new();
        let mut empty_price_leves = Vec::new();

        // For ask market order, match against bids (in desc order)
        for (&Reverse(price), limit) in &mut self.bids {
            if order.is_filled() {
                break;
            }

            let mut limit_matches = limit.fill(order);
            let sized_filled: Decimal = limit_matches.iter().map(|m| m.size_filled).sum();
            self.bid_total_volume -= sized_filled;
            matches.append(&mut limit_matches);

            if limit.is_empty() {
                empty_price_leves.push(price);
            }
        }

        for price in empty_price_leves {
            self.bids.remove(&Reverse(price));
        }

        Ok(matches)
    }

    pub fn place_limit_order(&mut self, price: Decimal, order: Order) {
        self.order_index.insert(order.id, (order.side, price));

        match order.side {
            Side::Ask => {
                self.ask_total_volume += order.size;
                self.asks
                    .entry(price)
                    .or_insert_with(|| Limit::new(price))
                    .add_order(order);
            }
            Side::Bid => {
                self.bid_total_volume += order.size;
                self.bids
                    .entry(Reverse(price))
                    .or_insert_with(|| Limit::new(price))
                    .add_order(order);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_market_bid_fully_matches_limit_ask() {
        let mut order_book = OrderBook::new();

        let ask_price = dec!(100.0);
        let ask_order = Order::ask(dec!(5.0));
        let ask_order_id = ask_order.id;

        order_book.place_limit_order(ask_price, ask_order);

        assert_eq!(order_book.ask_total_volume, dec!(5.0));

        let mut market_bid_order = Order::bid(dec!(5.0));
        let market_bid_order_id = market_bid_order.id;

        let result = order_book.place_market_order(&mut market_bid_order);
        assert!(result.is_ok());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 1);

        let market_match = &matches[0];
        assert_eq!(market_match.bid.id, market_bid_order_id);
        assert_eq!(market_match.ask.id, ask_order_id);
        assert_eq!(market_match.size_filled, dec!(5.0));
        assert_eq!(market_match.price, ask_price);

        assert!(market_bid_order.is_filled());
        assert_eq!(order_book.ask_total_volume, dec!(0.0));
        assert_eq!(order_book.asks.len(), 0);
    }

    #[test]
    fn test_market_ask_partially_matches_multiple_limits() {
        let mut order_book = OrderBook::new();

        let bid_price1 = dec!(102.0);
        let bid_price2 = dec!(101.0);
        let bid_price3 = dec!(100.0);

        let bid_order1 = Order::bid(dec!(3.0));
        let bid_order2 = Order::bid(dec!(2.0));
        let bid_order3 = Order::bid(dec!(4.0));

        let bid_id1 = bid_order1.id;
        let bid_id2 = bid_order2.id;

        order_book.place_limit_order(bid_price1, bid_order1);
        order_book.place_limit_order(bid_price2, bid_order2);
        order_book.place_limit_order(bid_price3, bid_order3);
        assert_eq!(order_book.bid_total_volume, dec!(9.0));

        let mut market_order = Order::ask(dec!(5.0));

        let result = order_book.place_market_order(&mut market_order);

        assert!(result.is_ok());
        let matches = result.unwrap();
        assert_eq!(matches.len(), 2); // Should match against two highest bids

        assert_eq!(matches[0].price, bid_price1);
        assert_eq!(matches[0].bid.id, bid_id1);
        assert_eq!(matches[0].size_filled, dec!(3.0));

        assert_eq!(matches[1].price, bid_price2);
        assert_eq!(matches[1].bid.id, bid_id2);
        assert_eq!(matches[1].size_filled, dec!(2.0));

        assert!(market_order.is_filled());
        assert_eq!(order_book.bid_total_volume, dec!(4.0)); // Only the lowest bid remains
        assert_eq!(order_book.bids.len(), 1);
        assert!(order_book.bids.contains_key(&Reverse(bid_price3)));
    }

    #[test]
    fn test_market_order_fails_with_insufficient_volume() {
        let mut orderbook = OrderBook::new();

        let ask_price = dec!(100.0);
        let ask_order = Order::ask(dec!(2.0));

        orderbook.place_limit_order(ask_price, ask_order);
        assert_eq!(orderbook.ask_total_volume, dec!(2.0));

        let mut market_order = Order::bid(dec!(5.0));

        let result = orderbook.place_market_order(&mut market_order);

        assert!(result.is_err());

        match result {
            Err(OrderBookError::NotEnoughVolume {
                side,
                expected_volume,
                actual_volume,
            }) => {
                assert_eq!(side, Side::Bid);
                assert_eq!(expected_volume, dec!(5.0));
                assert_eq!(actual_volume, dec!(2.0));
            }
            _ => panic!("Expected NotEnoughVolume error"),
        }

        assert_eq!(orderbook.ask_total_volume, dec!(2.0));
        assert_eq!(orderbook.asks.len(), 1);
        assert!(!market_order.is_filled());
    }

    #[test]
    fn test_place_single_bid_limit_order() {
        let mut order_book = OrderBook::new();

        let price = dec!(100);
        let bid_order = Order::bid(dec!(5));
        let bid_order_id = bid_order.id;

        order_book.place_limit_order(price, bid_order);

        assert_eq!(order_book.bid_total_volume, dec!(5));
        assert_eq!(order_book.ask_total_volume, dec!(0));

        assert!(order_book.bids.contains_key(&Reverse(price)));
        assert!(order_book.order_index.contains_key(&bid_order_id));

        assert_eq!(order_book.bids.len(), 1);
        assert_eq!(order_book.asks.len(), 0);

        let limit = order_book.bids.get(&Reverse(price)).unwrap();
        assert_eq!(limit.price, price);
        assert_eq!(limit.total_volume, dec!(5));
        assert!(limit.orders_by_uuid.contains_key(&bid_order_id));
        assert_eq!(limit.orders_by_timestamp.len(), 1);
    }

    #[test]
    fn test_place_multiple_ask_limit_orders_at_same_price() {
        let mut order_book = OrderBook::new();

        let price = dec!(50);

        let ask_order1 = Order::ask(dec!(2.0));
        let ask_order2 = Order::ask(dec!(3.0));
        let ask_order3 = Order::ask(dec!(1.5));

        let ask_order1_id = ask_order1.id;
        let ask_order2_id = ask_order2.id;
        let ask_order3_id = ask_order3.id;

        order_book.place_limit_order(price, ask_order1);
        order_book.place_limit_order(price, ask_order2);
        order_book.place_limit_order(price, ask_order3);

        assert_eq!(order_book.ask_total_volume, dec!(6.5));
        assert_eq!(order_book.bids.len(), 0);
        assert!(order_book.asks.contains_key(&price));

        assert!(order_book.order_index.contains_key(&ask_order1_id));
        assert!(order_book.order_index.contains_key(&ask_order2_id));
        assert!(order_book.order_index.contains_key(&ask_order3_id));

        let limit = order_book.asks.get(&price).unwrap();

        assert_eq!(limit.total_volume, dec!(6.5));
        assert_eq!(limit.price, price);
        assert!(limit.orders_by_uuid.contains_key(&ask_order1_id));
        assert!(limit.orders_by_uuid.contains_key(&ask_order2_id));
        assert!(limit.orders_by_uuid.contains_key(&ask_order3_id));
        assert_eq!(limit.orders_by_timestamp.len(), 3);
    }

    #[test]
    fn test_place_multiple_limit_orders_at_multiple_price_levels() {
        let mut order_book = OrderBook::new();

        let bid_price1 = dec!(90.0);
        let bid_price2 = dec!(95.0);

        let ask_price1 = dec!(110.0);

        let bid_order1 = Order::bid(dec!(1.0));
        let bid_order2 = Order::bid(dec!(2.0));
        let bid_order3 = Order::bid(dec!(3.0));

        let ask_order = Order::ask(dec!(3.0));

        order_book.place_limit_order(bid_price1, bid_order1);
        order_book.place_limit_order(bid_price2, bid_order2);
        order_book.place_limit_order(bid_price2, bid_order3);
        order_book.place_limit_order(ask_price1, ask_order);

        assert_eq!(order_book.bid_total_volume, dec!(6.0));
        assert_eq!(order_book.ask_total_volume, dec!(3.0));

        assert_eq!(order_book.bids.len(), 2);
        assert_eq!(order_book.asks.len(), 1);

        assert!(order_book.bids.contains_key(&Reverse(bid_price1)));
        assert!(order_book.bids.contains_key(&Reverse(bid_price2)));
        assert!(order_book.asks.contains_key(&ask_price1));

        assert_eq!(
            order_book
                .bids
                .get(&Reverse(bid_price1))
                .unwrap()
                .total_volume,
            dec!(1.0)
        );
        assert_eq!(
            order_book
                .bids
                .get(&Reverse(bid_price2))
                .unwrap()
                .total_volume,
            dec!(5.0)
        );
        assert_eq!(
            order_book.asks.get(&ask_price1).unwrap().total_volume,
            dec!(3.0)
        );
    }

    #[test]
    fn test_cancel_bid_order() {
        let mut order_book = OrderBook::new();
        let price = dec!(100.0);
        let bid_order = Order::bid(dec!(5.0));
        let bid_order_id = bid_order.id;

        order_book.place_limit_order(price, bid_order);
        assert_eq!(order_book.bid_total_volume, dec!(5.0));
        assert_eq!(order_book.bids.len(), 1);

        let cancelled_order = order_book.cancel_order(bid_order_id).unwrap();

        assert_eq!(cancelled_order.id, bid_order_id);
        assert_eq!(order_book.bid_total_volume, dec!(0.0));
        assert_eq!(order_book.bids.len(), 0);
    }

    #[test]
    fn test_cancel_ask_order() {
        let mut order_book = OrderBook::new();
        let price = dec!(100.0);
        let ask_order = Order::ask(dec!(5.0));
        let ask_order_id = ask_order.id;

        order_book.place_limit_order(price, ask_order);
        assert_eq!(order_book.ask_total_volume, dec!(5.0));

        let cancelled_order = order_book.cancel_order(ask_order_id).unwrap();

        assert_eq!(cancelled_order.id, ask_order_id);
        assert_eq!(order_book.ask_total_volume, dec!(0.0));
        assert_eq!(order_book.asks.len(), 0);
    }

    #[test]
    fn test_cancel_one_of_multiple_orders_at_price_level() {
        let mut order_book = OrderBook::new();
        let price = dec!(100.0);

        let order1 = Order::bid(dec!(1.0));
        let order2 = Order::bid(dec!(2.0));
        let order3 = Order::bid(dec!(3.0));

        let id1 = order1.id;
        let id2 = order2.id;
        let id3 = order3.id;

        order_book.place_limit_order(price, order1);
        order_book.place_limit_order(price, order2);
        order_book.place_limit_order(price, order3);

        assert_eq!(order_book.bid_total_volume, dec!(6.0));

        let cancelled_order = order_book.cancel_order(id2).unwrap();

        assert_eq!(cancelled_order.id, id2);
        assert_eq!(order_book.bid_total_volume, dec!(4.0)); // 6.0 - 2.0 = 4.0
        assert_eq!(order_book.bids.len(), 1);

        let key = Reverse(price);
        let limit = order_book.bids.get(&key).unwrap();
        assert_eq!(limit.orders_by_uuid.len(), 2);
        assert!(limit.orders_by_uuid.contains_key(&id1));
        assert!(limit.orders_by_uuid.contains_key(&id3));
    }
}
