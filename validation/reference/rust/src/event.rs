//! The events the engine writes, and their exact line format.

use std::fmt;

pub type OrderId = u64;
pub type Price = u64;
pub type Qty = u64;

/// A symbol or owner name: 1 to 8 bytes of `A-Z` and `0-9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name {
    bytes: [u8; Name::MAX_LEN],
    len: u8,
}

impl Name {
    pub const MAX_LEN: usize = 8;

    /// Returns `None` when `bytes` is not a valid name.
    pub fn parse(bytes: &[u8]) -> Option<Name> {
        let is_valid = (1..=Name::MAX_LEN).contains(&bytes.len())
            && bytes
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if !is_valid {
            return None;
        }
        let mut name = Name {
            bytes: [0; Name::MAX_LEN],
            len: u8::try_from(bytes.len()).ok()?,
        };
        name.bytes[..bytes.len()].copy_from_slice(bytes);
        Some(name)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)])
            .expect("a name holds only ASCII letters and digits")
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }

    /// Whether an order of this side with price `limit` can trade with a resting order at `price`.
    pub fn can_trade_at(self, limit: Price, price: Price) -> bool {
        match self {
            Side::Buy => price <= limit,
            Side::Sell => price >= limit,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::Buy => "B",
            Side::Sell => "S",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectCode {
    UnknownCommand,
    Malformed,
    UnknownSymbol,
    DuplicateSymbol,
    DuplicateOrderId,
    UnknownOrder,
    InvalidPrice,
    InvalidQty,
    WouldTrade,
}

impl fmt::Display for RejectCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RejectCode::UnknownCommand => "UNKNOWN_COMMAND",
            RejectCode::Malformed => "MALFORMED",
            RejectCode::UnknownSymbol => "UNKNOWN_SYMBOL",
            RejectCode::DuplicateSymbol => "DUPLICATE_SYMBOL",
            RejectCode::DuplicateOrderId => "DUPLICATE_ORDER_ID",
            RejectCode::UnknownOrder => "UNKNOWN_ORDER",
            RejectCode::InvalidPrice => "INVALID_PRICE",
            RejectCode::InvalidQty => "INVALID_QTY",
            RejectCode::WouldTrade => "WOULD_TRADE",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutReason {
    Cancelled,
    Unfilled,
    SelfTrade,
}

impl fmt::Display for OutReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OutReason::Cancelled => "CANCELLED",
            OutReason::Unfilled => "UNFILLED",
            OutReason::SelfTrade => "SELF_TRADE",
        })
    }
}

/// The best price of one side of a book and the displayed quantity at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BestLevel {
    pub price: Price,
    pub qty: Qty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Symbol(Name),
    Accept(OrderId),
    Reject {
        line: u64,
        code: RejectCode,
    },
    Trade {
        symbol: Name,
        price: Price,
        qty: Qty,
        taker: OrderId,
        maker: OrderId,
    },
    Out {
        id: OrderId,
        qty: Qty,
        reason: OutReason,
    },
    Amend {
        id: OrderId,
        price: Price,
        qty: Qty,
    },
    Top {
        symbol: Name,
        bid: Option<BestLevel>,
        ask: Option<BestLevel>,
    },
    Depth {
        symbol: Name,
        bid_levels: usize,
        ask_levels: usize,
    },
    Level {
        side: Side,
        price: Price,
        qty: Qty,
        orders: usize,
    },
}

/// Writes `price,qty` of a best level, or `0,0` for an empty side.
struct TopSide(Option<BestLevel>);

impl fmt::Display for TopSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let BestLevel { price, qty } = self.0.unwrap_or(BestLevel { price: 0, qty: 0 });
        write!(f, "{price},{qty}")
    }
}

/// Writes the event as one protocol line, without its LF.
impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Event::Symbol(symbol) => write!(f, "SYMBOL,{symbol}"),
            Event::Accept(id) => write!(f, "ACCEPT,{id}"),
            Event::Reject { line, code } => write!(f, "REJECT,{line},{code}"),
            Event::Trade {
                symbol,
                price,
                qty,
                taker,
                maker,
            } => write!(f, "TRADE,{symbol},{price},{qty},{taker},{maker}"),
            Event::Out { id, qty, reason } => write!(f, "OUT,{id},{qty},{reason}"),
            Event::Amend { id, price, qty } => write!(f, "AMEND,{id},{price},{qty}"),
            Event::Top { symbol, bid, ask } => {
                write!(f, "TOP,{symbol},{},{}", TopSide(bid), TopSide(ask))
            }
            Event::Depth {
                symbol,
                bid_levels,
                ask_levels,
            } => write!(f, "DEPTH,{symbol},{bid_levels},{ask_levels}"),
            Event::Level {
                side,
                price,
                qty,
                orders,
            } => write!(f, "LEVEL,{side},{price},{qty},{orders}"),
        }
    }
}
