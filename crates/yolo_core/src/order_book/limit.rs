use std::collections::{BTreeSet, HashMap};

use rust_decimal::{Decimal, dec};
use uuid::Uuid;

use crate::order_book::Side;

use super::{
    OrderMatch,
    order::{Order, OrderByTimestamp},
};

#[derive(Debug)]
pub struct Limit {
    pub price: Decimal,
    pub orders_by_uuid: HashMap<Uuid, Order>,
    pub orders_by_timestamp: BTreeSet<OrderByTimestamp>,
    pub total_volume: Decimal,
}

impl Limit {
    pub fn new(price: Decimal) -> Self {
        Self {
            price,
            orders_by_uuid: HashMap::new(),
            orders_by_timestamp: BTreeSet::new(),
            total_volume: dec!(0.0),
        }
    }

    pub fn add_order(&mut self, order: Order) {
        self.orders_by_uuid.insert(order.id, order.clone());
        self.orders_by_timestamp
            .insert(OrderByTimestamp(order.clone()));
        self.total_volume += order.size;
    }

    pub fn remove_order(&mut self, id: Uuid) -> Option<Order> {
        if let Some(order) = self.orders_by_uuid.remove(&id) {
            self.orders_by_timestamp
                .remove(&OrderByTimestamp(order.clone()));
            self.total_volume -= order.size;
            Some(order)
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.orders_by_uuid.is_empty()
    }

    pub fn fill(&mut self, order: &mut Order) -> Vec<OrderMatch> {
        let mut matches = Vec::new();
        let mut filled_order_ids: Vec<Uuid> = Vec::new();

        for (&limit_order_id, limit_order) in self.orders_by_uuid.iter_mut() {
            let orders_match = Self::match_orders(order, limit_order, self.price);
            matches.push(orders_match);

            if limit_order.is_filled() {
                filled_order_ids.push(limit_order_id)
            }

            if order.is_filled() {
                break;
            }
        }

        for id in filled_order_ids {
            self.remove_order(id);
        }

        matches
    }

    fn match_orders(order1: &mut Order, order2: &mut Order, price: Decimal) -> OrderMatch {
        let (bid, ask) = match (order1.side, order2.side) {
            (Side::Bid, Side::Ask) => (order1, order2),
            (Side::Ask, Side::Bid) => (order2, order1),
            (_, _) => unreachable!(),
        };

        let size_filled = if ask.size >= bid.size {
            ask.size -= bid.size;
            let size = bid.size;
            bid.size = dec!(0);
            size
        } else {
            bid.size -= ask.size;
            let size = ask.size;
            ask.size = dec!(0);
            size
        };

        OrderMatch {
            ask: ask.clone(),
            bid: bid.clone(),
            size_filled,
            price,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order_book::order::{Order, Side};

    #[test]
    fn test_add_and_remove_order() {
        let mut limit = Limit::new(dec!(42.0));
        let order1 = Order::bid(dec!(1.0));
        let order2 = Order::ask(dec!(2.5));

        limit.add_order(order1.clone());
        limit.add_order(order2.clone());

        assert_eq!(limit.orders_by_uuid.len(), 2);
        assert_eq!(limit.orders_by_timestamp.len(), 2);
        assert_eq!(limit.total_volume, order1.size + order2.size);

        let removed1 = limit.remove_order(order1.id);
        assert_eq!(removed1, Some(order1));
        assert_eq!(limit.orders_by_uuid.len(), 1);
        assert_eq!(limit.orders_by_timestamp.len(), 1);
        assert_eq!(limit.total_volume, order2.size);

        let removed2 = limit.remove_order(order2.id);
        assert_eq!(removed2, Some(order2));
        assert_eq!(limit.orders_by_uuid.len(), 0);
        assert_eq!(limit.orders_by_timestamp.len(), 0);
        assert_eq!(limit.total_volume, dec!(0));
    }

    #[test]
    fn test_orders_by_timestamp_are_sorted() {
        let mut limit = Limit::new(dec!(100));
        let order1 = Order {
            id: Uuid::new_v4(),
            size: dec!(1.0),
            side: Side::Bid,
            timestamp: 5,
        };
        let order2 = Order {
            id: Uuid::new_v4(),
            size: dec!(2.0),
            side: Side::Ask,
            timestamp: 2,
        };
        let order3 = Order {
            id: Uuid::new_v4(),
            size: dec!(3.0),
            side: Side::Bid,
            timestamp: 3,
        };
        let order4 = Order {
            id: Uuid::new_v4(),
            size: dec!(4.0),
            side: Side::Ask,
            timestamp: 7,
        };

        limit.add_order(order1.clone());
        limit.add_order(order2.clone());
        limit.add_order(order3.clone());
        limit.add_order(order4.clone());

        limit.remove_order(order1.id);
        limit.remove_order(order3.id);

        let timestamps = limit
            .orders_by_timestamp
            .iter()
            .map(|o| o.0.timestamp)
            .collect::<Vec<_>>();

        assert_eq!(timestamps, vec![2, 7]);
    }
}
