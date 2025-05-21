use std::fmt::Display;

use rust_decimal::{Decimal, dec};
use uuid::Uuid;

use crate::time::timestamp;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

impl Side {
    pub fn opposite(&self) -> Side {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

impl Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Bid => write!(f, "bid"),
            Side::Ask => write!(f, "ask"),
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub struct Order {
    pub id: Uuid,
    pub size: Decimal,
    pub side: Side,
    pub timestamp: i64,
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderByTimestamp(pub Order);

impl Ord for OrderByTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .timestamp
            .cmp(&other.0.timestamp)
            .then(self.0.id.cmp(&other.0.id))
    }
}

impl PartialOrd for OrderByTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Order {
    pub fn new(side: Side, size: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            side,
            size,
            timestamp: timestamp(),
        }
    }

    pub fn bid(size: Decimal) -> Self {
        Self::new(Side::Bid, size)
    }

    pub fn ask(size: Decimal) -> Self {
        Self::new(Side::Ask, size)
    }

    pub fn is_filled(&self) -> bool {
        self.size == dec!(0)
    }
}
