mod limit;
mod order;

pub use limit::*;
pub use order::*;

use rust_decimal::{Decimal, dec};
use std::collections::{BTreeMap, HashMap};
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
    bids: BTreeMap<Decimal, Limit>,
    ask_total_volume: Decimal,
    bid_total_volume: Decimal,
    order_locations: HashMap<Uuid, (Side, Decimal)>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
            ask_total_volume: dec!(0),
            bid_total_volume: dec!(0),
            order_locations: HashMap::new(),
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
        if let Some((side, price)) = self.order_locations.remove(&id) {
            let limits_by_price = match side {
                Side::Bid => &mut self.bids,
                Side::Ask => &mut self.asks,
            };

            if let Some(limit) = limits_by_price.get_mut(&price) {
                if let Some(removed_order) = limit.remove_order(id) {
                    match side {
                        Side::Bid => self.bid_total_volume -= removed_order.size,
                        Side::Ask => self.ask_total_volume -= removed_order.size,
                    }

                    Ok(removed_order)
                } else {
                    Err(OrderBookError::InconsistentState)
                }
            } else {
                Err(OrderBookError::LimitNotFound(price))
            }
        } else {
            Err(OrderBookError::OrderNotFound(id))
        }
    }

    pub fn place_market_order(&mut self, order: &mut Order) -> Result<Vec<Match>, OrderBookError> {
        self.ensure_volume(order)?;

        let mut matches = Vec::new();
        let mut empty_limit_prices = Vec::new();

        let (opposite_limits, opposite_side) = match order.side {
            Side::Bid => (&mut self.asks, Side::Bid),
            Side::Ask => (&mut self.bids, Side::Ask),
        };

        for limit in opposite_limits.values_mut() {
            let mut limit_matches = limit.fill(order);
            matches.append(&mut limit_matches);

            if limit.is_empty() {
                empty_limit_prices.push(limit.price);
            }
        }

        for price in empty_limit_prices {
            self.remove_limit(opposite_side, price);
        }

        Ok(matches)
    }

    pub fn place_limit_order(&mut self, price: Decimal, order: Order) {
        match order.side {
            Side::Ask => {
                self.ask_total_volume += order.size;
                &mut self.asks
            }
            Side::Bid => {
                self.bid_total_volume += order.size;
                &mut self.bids
            }
        }
        .entry(price)
        .or_insert_with(|| Limit::new(price))
        .add_order(order.clone());

        self.order_locations.insert(order.id, (order.side, price));
    }

    pub fn remove_limit(&mut self, side: Side, price: Decimal) {
        let (limits, side_total_volume) = match side {
            Side::Bid => (&mut self.bids, &mut self.ask_total_volume),
            Side::Ask => (&mut self.asks, &mut self.bid_total_volume),
        };

        if let Some(limit) = limits.remove(&price) {
            *side_total_volume -= limit.total_volume;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place_single_bid_limit_order() {
        let mut order_book = OrderBook::new();

        let price = dec!(100);
        let bid_order = Order::bid(dec!(5));
        let bid_order_id = bid_order.id;

        order_book.place_limit_order(price, bid_order);

        assert_eq!(order_book.bid_total_volume, dec!(5));
        assert_eq!(order_book.ask_total_volume, dec!(0));

        assert!(order_book.bids.contains_key(&price));
        assert!(order_book.order_locations.contains_key(&bid_order_id));

        assert_eq!(order_book.bids.len(), 1);
        assert_eq!(order_book.asks.len(), 0);

        let limit = order_book.bids.get(&price).unwrap();
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

        assert!(order_book.order_locations.contains_key(&ask_order1_id));
        assert!(order_book.order_locations.contains_key(&ask_order2_id));
        assert!(order_book.order_locations.contains_key(&ask_order3_id));

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

        let ask_order = Order::ask(dec!(3.0));

        order_book.place_limit_order(bid_price1, bid_order1);
        order_book.place_limit_order(bid_price2, bid_order2);
        order_book.place_limit_order(ask_price1, ask_order);

        assert_eq!(order_book.bid_total_volume, dec!(3.0));
        assert_eq!(order_book.ask_total_volume, dec!(3.0));

        assert_eq!(order_book.bids.len(), 2);
        assert_eq!(order_book.asks.len(), 1);

        assert!(order_book.bids.contains_key(&bid_price1));
        assert!(order_book.bids.contains_key(&bid_price2));
        assert!(order_book.asks.contains_key(&ask_price1));

        assert_eq!(
            order_book.bids.get(&bid_price1).unwrap().total_volume,
            dec!(1.0)
        );
        assert_eq!(
            order_book.bids.get(&bid_price2).unwrap().total_volume,
            dec!(2.0)
        );
        assert_eq!(
            order_book.asks.get(&ask_price1).unwrap().total_volume,
            dec!(3.0)
        );
    }

    #[test]
    fn test_place_multiple_ask_limit_orders() {
        let mut order_book = OrderBook::new();

        let ask_order_1 = Order::ask(dec!(10));
        let ask_order_2 = Order::ask(dec!(5));

        order_book.place_limit_order(dec!(10_000), ask_order_1);
        order_book.place_limit_order(dec!(9_000), ask_order_2);
    }
}
