//! A bid book that keeps orders at each price in arrival order, so the oldest order at a price fills first.

use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderId(pub u64);

#[derive(Debug, PartialEq, Eq)]
pub enum BookError {
    DuplicateOrder(OrderId),
    UnknownOrder(OrderId),
}

#[derive(Default)]
pub struct Book {
    bids: BTreeMap<Price, VecDeque<(OrderId, Quantity)>>,
}

impl Book {
    pub fn add_bid(&mut self, id: OrderId, price: Price, quantity: Quantity) -> Result<(), BookError> {
        if self.contains(id) {
            return Err(BookError::DuplicateOrder(id));
        }
        self.bids.entry(price).or_default().push_back((id, quantity));
        Ok(())
    }

    pub fn cancel(&mut self, id: OrderId) -> Result<Quantity, BookError> {
        for queue in self.bids.values_mut() {
            if let Some(position) = queue.iter().position(|(queued, _)| *queued == id) {
                let (_, quantity) = queue.remove(position).expect("the position was just found");
                return Ok(quantity);
            }
        }
        Err(BookError::UnknownOrder(id))
    }

    fn contains(&self, id: OrderId) -> bool {
        self.bids.values().flatten().any(|(queued, _)| *queued == id)
    }
}
