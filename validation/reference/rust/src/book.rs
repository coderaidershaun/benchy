//! The order book of one symbol: price levels in price-time priority, matching, and queries.

use std::collections::{BTreeMap, HashMap};

use crate::event::{BestLevel, Event, Name, OrderId, OutReason, Price, Qty, Side};

/// The position of an order in its queue. A later arrival gets a larger number.
type Sequence = u64;

#[derive(Debug, Default)]
struct Level {
    queue: BTreeMap<Sequence, OrderId>,
    displayed: Qty,
}

#[derive(Debug)]
struct RestingOrder {
    side: Side,
    price: Price,
    displayed: Qty,
    hidden: Qty,
    /// The peak size of an iceberg; `None` for an order that shows all of it.
    peak: Option<Qty>,
    owner: Name,
    sequence: Sequence,
}

impl RestingOrder {
    fn open(&self) -> Qty {
        self.displayed + self.hidden
    }
}

/// An order that matches against the book.
#[derive(Debug, Clone, Copy)]
pub struct Taker {
    pub id: OrderId,
    pub side: Side,
    /// The worst price the order accepts; `None` for a market order.
    pub limit: Option<Price>,
    pub owner: Name,
}

impl Taker {
    fn can_trade_at(&self, price: Price) -> bool {
        self.limit
            .is_none_or(|limit| self.side.can_trade_at(limit, price))
    }
}

#[derive(Debug)]
pub struct Book {
    symbol: Name,
    pub tick: Price,
    pub lot: Qty,
    bids: BTreeMap<Price, Level>,
    asks: BTreeMap<Price, Level>,
    orders: HashMap<OrderId, RestingOrder>,
    next_sequence: Sequence,
}

impl Book {
    pub fn new(symbol: Name, tick: Price, lot: Qty) -> Book {
        Book {
            symbol,
            tick,
            lot,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            next_sequence: 0,
        }
    }

    pub fn side_of(&self, id: OrderId) -> Option<Side> {
        self.orders.get(&id).map(|order| order.side)
    }

    /// Whether an order of `side` at `price` would trade with the best opposite price at once.
    pub fn would_trade(&self, side: Side, price: Price) -> bool {
        self.best_price(side.opposite())
            .is_some_and(|best| side.can_trade_at(price, best))
    }

    /// Whether the opposite orders that `taker` can reach, other than its owner's, hold `qty` in all.
    pub fn can_fill(&self, taker: &Taker, qty: Qty) -> bool {
        let mut available = 0;
        for (&price, level) in self.levels_in_priority(taker.side.opposite()) {
            if !taker.can_trade_at(price) {
                break;
            }
            for id in level.queue.values() {
                let order = &self.orders[id];
                if order.owner != taker.owner {
                    available += order.open();
                    if available >= qty {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Matches `taker` with `qty` open and returns the quantity left.
    pub fn fill(&mut self, taker: &Taker, mut qty: Qty, events: &mut Vec<Event>) -> Qty {
        let maker_side = taker.side.opposite();
        while qty > 0 {
            let Some(price) = self.best_price(maker_side) else {
                break;
            };
            if !taker.can_trade_at(price) {
                break;
            }
            let maker_id = self.front_of(maker_side, price);
            let maker = &self.orders[&maker_id];
            if maker.owner == taker.owner {
                let open = self.remove(maker_id);
                events.push(Event::Out {
                    id: maker_id,
                    qty: open,
                    reason: OutReason::SelfTrade,
                });
                continue;
            }
            let traded = qty.min(maker.displayed);
            qty -= traded;
            events.push(Event::Trade {
                symbol: self.symbol,
                price,
                qty: traded,
                taker: taker.id,
                maker: maker_id,
            });
            self.take_displayed(maker_id, traded);
        }
        qty
    }

    /// Rests a new order. An iceberg passes its peak size.
    pub fn rest(&mut self, taker: &Taker, price: Price, qty: Qty, peak: Option<Qty>) {
        let displayed = peak.map_or(qty, |peak| peak.min(qty));
        let order = RestingOrder {
            side: taker.side,
            price,
            displayed,
            hidden: qty - displayed,
            peak,
            owner: taker.owner,
            sequence: 0,
        };
        self.orders.insert(taker.id, order);
        self.join_back(taker.id);
    }

    /// Removes a live order and returns its open quantity.
    pub fn cancel(&mut self, id: OrderId) -> Option<Qty> {
        self.orders.contains_key(&id).then(|| self.remove(id))
    }

    /// Sets a new price and open quantity. The caller has checked that the order is live and
    /// would not trade at `price`.
    pub fn amend(&mut self, id: OrderId, price: Price, qty: Qty) {
        let order = &self.orders[&id];
        let keeps_priority = price == order.price && qty <= order.open();
        if keeps_priority {
            let displayed = order.displayed.min(qty);
            let shown_less = order.displayed - displayed;
            let (side, price) = (order.side, order.price);
            let order = self.orders.get_mut(&id).expect("the order is live");
            order.displayed = displayed;
            order.hidden = qty - displayed;
            self.levels_mut(side)
                .get_mut(&price)
                .expect("a live order has a level")
                .displayed -= shown_less;
        } else {
            self.leave_level(id);
            let order = self.orders.get_mut(&id).expect("the order is live");
            order.price = price;
            order.displayed = order.peak.map_or(qty, |peak| peak.min(qty));
            order.hidden = qty - order.displayed;
            self.join_back(id);
        }
    }

    pub fn best(&self, side: Side) -> Option<BestLevel> {
        let price = self.best_price(side)?;
        Some(BestLevel {
            price,
            qty: self.levels(side)[&price].displayed,
        })
    }

    /// Writes `DEPTH` and then up to `levels` `LEVEL` events for each side.
    pub fn depth(&self, levels: u64, events: &mut Vec<Event>) {
        let limit = usize::try_from(levels).unwrap_or(usize::MAX);
        let bid_levels = self.bids.len().min(limit);
        let ask_levels = self.asks.len().min(limit);
        events.push(Event::Depth {
            symbol: self.symbol,
            bid_levels,
            ask_levels,
        });
        for (side, count) in [(Side::Buy, bid_levels), (Side::Sell, ask_levels)] {
            for (&price, level) in self.levels_in_priority(side).take(count) {
                events.push(Event::Level {
                    side,
                    price,
                    qty: level.displayed,
                    orders: level.queue.len(),
                });
            }
        }
    }

    fn levels(&self, side: Side) -> &BTreeMap<Price, Level> {
        match side {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        }
    }

    fn levels_mut(&mut self, side: Side) -> &mut BTreeMap<Price, Level> {
        match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        }
    }

    /// The levels of one side, best price first.
    fn levels_in_priority(&self, side: Side) -> Box<dyn Iterator<Item = (&Price, &Level)> + '_> {
        match side {
            Side::Buy => Box::new(self.bids.iter().rev()),
            Side::Sell => Box::new(self.asks.iter()),
        }
    }

    fn best_price(&self, side: Side) -> Option<Price> {
        let best = match side {
            Side::Buy => self.bids.last_key_value(),
            Side::Sell => self.asks.first_key_value(),
        };
        best.map(|(&price, _)| price)
    }

    fn front_of(&self, side: Side, price: Price) -> OrderId {
        let (_, &id) = self.levels(side)[&price]
            .queue
            .first_key_value()
            .expect("an empty level is always removed");
        id
    }

    /// Trades `qty` of a resting order's displayed part, then refills it or removes it.
    fn take_displayed(&mut self, id: OrderId, qty: Qty) {
        let order = self.orders.get_mut(&id).expect("the maker is live");
        order.displayed -= qty;
        let (side, price, displayed, hidden) =
            (order.side, order.price, order.displayed, order.hidden);
        self.levels_mut(side)
            .get_mut(&price)
            .expect("a live order has a level")
            .displayed -= qty;
        if displayed > 0 {
            return;
        }
        if hidden == 0 {
            self.remove(id);
            return;
        }
        self.leave_level(id);
        let order = self.orders.get_mut(&id).expect("the maker is live");
        let refill = order.peak.map_or(hidden, |peak| peak.min(hidden));
        order.displayed = refill;
        order.hidden -= refill;
        self.join_back(id);
    }

    fn remove(&mut self, id: OrderId) -> Qty {
        self.leave_level(id);
        self.orders
            .remove(&id)
            .expect("only live orders are removed")
            .open()
    }

    fn join_back(&mut self, id: OrderId) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let order = self.orders.get_mut(&id).expect("the order is live");
        order.sequence = sequence;
        let (side, price, displayed) = (order.side, order.price, order.displayed);
        let level = self.levels_mut(side).entry(price).or_default();
        level.queue.insert(sequence, id);
        level.displayed += displayed;
    }

    fn leave_level(&mut self, id: OrderId) {
        let order = &self.orders[&id];
        let (side, price, sequence, displayed) =
            (order.side, order.price, order.sequence, order.displayed);
        let levels = self.levels_mut(side);
        let level = levels.get_mut(&price).expect("a live order has a level");
        level.queue.remove(&sequence);
        level.displayed -= displayed;
        if level.queue.is_empty() {
            levels.remove(&price);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Name {
        Name::parse(text.as_bytes()).expect("test names are valid")
    }

    fn taker(id: OrderId, side: Side, limit: Option<Price>, owner: &str) -> Taker {
        Taker {
            id,
            side,
            limit,
            owner: name(owner),
        }
    }

    fn book() -> Book {
        Book::new(name("ACME"), 1, 1)
    }

    fn makers(events: &[Event]) -> Vec<(OrderId, Qty)> {
        events
            .iter()
            .filter_map(|event| match *event {
                Event::Trade { maker, qty, .. } => Some((maker, qty)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_refilled_iceberg_trades_after_the_orders_already_at_its_price() {
        let mut book = book();
        book.rest(&taker(1, Side::Sell, Some(100), "AL"), 100, 30, Some(10));
        book.rest(&taker(2, Side::Sell, Some(100), "BO"), 100, 5, None);
        let mut events = Vec::new();
        let left = book.fill(&taker(3, Side::Buy, Some(100), "CY"), 22, &mut events);
        assert_eq!(left, 0);
        assert_eq!(makers(&events), [(1, 10), (2, 5), (1, 7)]);
        assert_eq!(
            book.best(Side::Sell),
            Some(BestLevel { price: 100, qty: 3 })
        );
    }

    #[test]
    fn matching_stops_at_the_limit_price() {
        let mut book = book();
        book.rest(&taker(1, Side::Sell, Some(100), "AL"), 100, 10, None);
        book.rest(&taker(2, Side::Sell, Some(101), "AL"), 101, 10, None);
        let mut events = Vec::new();
        let left = book.fill(&taker(3, Side::Buy, Some(100), "BO"), 15, &mut events);
        assert_eq!(left, 5);
        assert_eq!(makers(&events), [(1, 10)]);
    }

    #[test]
    fn self_trade_removes_the_resting_order_and_keeps_the_incoming_quantity() {
        let mut book = book();
        book.rest(&taker(1, Side::Buy, Some(100), "AL"), 100, 30, Some(10));
        let mut events = Vec::new();
        let left = book.fill(&taker(2, Side::Sell, None, "AL"), 5, &mut events);
        assert_eq!(left, 5);
        assert_eq!(
            events,
            [Event::Out {
                id: 1,
                qty: 30,
                reason: OutReason::SelfTrade
            }]
        );
        assert_eq!(book.best(Side::Buy), None);
    }

    #[test]
    fn fill_or_kill_counts_hidden_quantity_but_not_its_own_owner() {
        let mut book = book();
        book.rest(&taker(1, Side::Sell, Some(100), "AL"), 100, 30, Some(10));
        book.rest(&taker(2, Side::Sell, Some(100), "BO"), 100, 50, None);
        assert!(book.can_fill(&taker(3, Side::Buy, Some(100), "CY"), 80));
        assert!(!book.can_fill(&taker(3, Side::Buy, Some(100), "CY"), 81));
        assert!(!book.can_fill(&taker(3, Side::Buy, Some(100), "AL"), 51));
        assert!(!book.can_fill(&taker(3, Side::Buy, Some(99), "CY"), 1));
    }

    #[test]
    fn amend_keeps_priority_only_for_the_same_price_and_no_larger_quantity() {
        let mut book = book();
        book.rest(&taker(1, Side::Buy, Some(100), "AL"), 100, 10, None);
        book.rest(&taker(2, Side::Buy, Some(100), "BO"), 100, 10, None);
        book.amend(1, 100, 10);
        let mut events = Vec::new();
        book.fill(&taker(3, Side::Sell, Some(100), "CY"), 1, &mut events);
        assert_eq!(makers(&events), [(1, 1)]);

        book.amend(1, 100, 10);
        events.clear();
        book.fill(&taker(4, Side::Sell, Some(100), "CY"), 1, &mut events);
        assert_eq!(makers(&events), [(2, 1)]);
    }

    #[test]
    fn amending_an_iceberg_down_shows_no_more_than_before() {
        let mut book = book();
        book.rest(&taker(1, Side::Sell, Some(100), "AL"), 100, 30, Some(10));
        book.amend(1, 100, 25);
        assert_eq!(
            book.best(Side::Sell),
            Some(BestLevel {
                price: 100,
                qty: 10
            })
        );
        book.amend(1, 100, 8);
        assert_eq!(
            book.best(Side::Sell),
            Some(BestLevel { price: 100, qty: 8 })
        );
        book.amend(1, 100, 40);
        assert_eq!(
            book.best(Side::Sell),
            Some(BestLevel {
                price: 100,
                qty: 10
            })
        );
        assert_eq!(book.cancel(1), Some(40));
        assert_eq!(book.best(Side::Sell), None);
    }
}
