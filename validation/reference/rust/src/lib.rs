//! A deterministic limit order book matching engine for several symbols, with no input or output.
//! Feed it one protocol line at a time with [`Engine::handle_line`] and write the events it returns.

mod book;
mod event;
mod parse;

use std::collections::HashMap;

use book::{Book, Taker};
pub use event::{BestLevel, Event, Name, OrderId, OutReason, Price, Qty, RejectCode, Side};
use parse::{Command, NewOrder, TimeInForce, parse};

#[derive(Debug, Default)]
pub struct Engine {
    line_number: u64,
    books: HashMap<Name, Book>,
    /// The symbol of every order id that was ever accepted.
    order_symbols: HashMap<OrderId, Name>,
}

/// What a new order does once it has matched on entry.
#[derive(Clone, Copy)]
enum Remainder {
    Rests { price: Price, peak: Option<Qty> },
    Drops,
}

impl Engine {
    pub fn new() -> Engine {
        Engine::default()
    }

    /// Handles the next input line, without its LF, and appends the events it writes.
    pub fn handle_line(&mut self, line: &[u8], events: &mut Vec<Event>) {
        self.line_number += 1;
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let outcome = parse(line).and_then(|command| self.execute(command, events));
        if let Err(code) = outcome {
            events.push(Event::Reject {
                line: self.line_number,
                code,
            });
        }
    }

    fn execute(&mut self, command: Command, events: &mut Vec<Event>) -> Result<(), RejectCode> {
        match command {
            Command::DefineSymbol { symbol, tick, lot } => {
                if self.books.contains_key(&symbol) {
                    return Err(RejectCode::DuplicateSymbol);
                }
                self.books.insert(symbol, Book::new(symbol, tick, lot));
                events.push(Event::Symbol(symbol));
            }
            Command::Limit { order, price, tif } => {
                let book = self.check_new_order(&order)?;
                check_multiple(price, book.tick, RejectCode::InvalidPrice)?;
                check_multiple(order.qty, book.lot, RejectCode::InvalidQty)?;
                if tif == TimeInForce::PostOnly && book.would_trade(order.side, price) {
                    return Err(RejectCode::WouldTrade);
                }
                let remainder = match tif {
                    TimeInForce::GoodTillCancel | TimeInForce::PostOnly => {
                        Remainder::Rests { price, peak: None }
                    }
                    TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill => Remainder::Drops,
                };
                let taker = taker(&order, Some(price));
                if tif == TimeInForce::FillOrKill && !book.can_fill(&taker, order.qty) {
                    self.accept(&order, events);
                    events.push(unfilled(order.id, order.qty));
                } else {
                    self.enter(&order, &taker, remainder, events);
                }
            }
            Command::Market { order } => {
                let book = self.check_new_order(&order)?;
                check_multiple(order.qty, book.lot, RejectCode::InvalidQty)?;
                self.enter(&order, &taker(&order, None), Remainder::Drops, events);
            }
            Command::Iceberg {
                order,
                price,
                display,
            } => {
                let book = self.check_new_order(&order)?;
                check_multiple(price, book.tick, RejectCode::InvalidPrice)?;
                check_multiple(order.qty, book.lot, RejectCode::InvalidQty)?;
                check_multiple(display, book.lot, RejectCode::InvalidQty)?;
                let remainder = Remainder::Rests {
                    price,
                    peak: Some(display),
                };
                self.enter(&order, &taker(&order, Some(price)), remainder, events);
            }
            Command::Cancel { id } => {
                let qty = self
                    .book_of_mut(id)
                    .and_then(|book| book.cancel(id))
                    .ok_or(RejectCode::UnknownOrder)?;
                events.push(Event::Out {
                    id,
                    qty,
                    reason: OutReason::Cancelled,
                });
            }
            Command::Amend { id, price, qty } => {
                let book = self.book_of_mut(id).ok_or(RejectCode::UnknownOrder)?;
                let side = book.side_of(id).ok_or(RejectCode::UnknownOrder)?;
                check_multiple(price, book.tick, RejectCode::InvalidPrice)?;
                check_multiple(qty, book.lot, RejectCode::InvalidQty)?;
                if book.would_trade(side, price) {
                    return Err(RejectCode::WouldTrade);
                }
                book.amend(id, price, qty);
                events.push(Event::Amend { id, price, qty });
            }
            Command::Top { symbol } => {
                let book = self.books.get(&symbol).ok_or(RejectCode::UnknownSymbol)?;
                events.push(Event::Top {
                    symbol,
                    bid: book.best(Side::Buy),
                    ask: book.best(Side::Sell),
                });
            }
            Command::Depth { symbol, levels } => {
                let book = self.books.get(&symbol).ok_or(RejectCode::UnknownSymbol)?;
                book.depth(levels, events);
            }
        }
        Ok(())
    }

    fn check_new_order(&self, order: &NewOrder) -> Result<&Book, RejectCode> {
        if self.order_symbols.contains_key(&order.id) {
            return Err(RejectCode::DuplicateOrderId);
        }
        self.books
            .get(&order.symbol)
            .ok_or(RejectCode::UnknownSymbol)
    }

    fn accept(&mut self, order: &NewOrder, events: &mut Vec<Event>) {
        self.order_symbols.insert(order.id, order.symbol);
        events.push(Event::Accept(order.id));
    }

    /// Accepts an order that passed its checks, matches it, and rests or drops what is left.
    fn enter(
        &mut self,
        order: &NewOrder,
        taker: &Taker,
        remainder: Remainder,
        events: &mut Vec<Event>,
    ) {
        self.accept(order, events);
        let book = self
            .books
            .get_mut(&order.symbol)
            .expect("the symbol was checked");
        let left = book.fill(taker, order.qty, events);
        if left == 0 {
            return;
        }
        match remainder {
            Remainder::Rests { price, peak } => book.rest(taker, price, left, peak),
            Remainder::Drops => events.push(unfilled(order.id, left)),
        }
    }

    fn book_of_mut(&mut self, id: OrderId) -> Option<&mut Book> {
        let symbol = self.order_symbols.get(&id)?;
        self.books.get_mut(symbol)
    }
}

fn taker(order: &NewOrder, limit: Option<Price>) -> Taker {
    Taker {
        id: order.id,
        side: order.side,
        limit,
        owner: order.owner,
    }
}

fn unfilled(id: OrderId, qty: Qty) -> Event {
    Event::Out {
        id,
        qty,
        reason: OutReason::Unfilled,
    }
}

fn check_multiple(value: u64, step: u64, code: RejectCode) -> Result<(), RejectCode> {
    if value.is_multiple_of(step) {
        Ok(())
    } else {
        Err(code)
    }
}
