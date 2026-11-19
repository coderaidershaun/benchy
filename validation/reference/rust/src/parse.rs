//! Turns one input line into a command, deciding every rejection that the line alone decides.

use crate::event::{Name, OrderId, Price, Qty, RejectCode, Side};

const MAX_LINE_BYTES: usize = 256;
const MAX_FIELDS: usize = 8;
const MAX_DIGITS: usize = 18;
const MAX_ID: u64 = 999_999_999_999_999_999;
const MAX_VALUE: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewOrder {
    pub id: OrderId,
    pub symbol: Name,
    pub side: Side,
    pub qty: Qty,
    pub owner: Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    DefineSymbol {
        symbol: Name,
        tick: Price,
        lot: Qty,
    },
    Limit {
        order: NewOrder,
        price: Price,
        tif: TimeInForce,
    },
    Market {
        order: NewOrder,
    },
    Iceberg {
        order: NewOrder,
        price: Price,
        display: Qty,
    },
    Cancel {
        id: OrderId,
    },
    Amend {
        id: OrderId,
        price: Price,
        qty: Qty,
    },
    Top {
        symbol: Name,
    },
    Depth {
        symbol: Name,
        levels: u64,
    },
}

/// Parses a line whose LF and trailing CR are already removed.
pub fn parse(line: &[u8]) -> Result<Command, RejectCode> {
    if line.len() > MAX_LINE_BYTES {
        return Err(RejectCode::Malformed);
    }
    let mut fields: [&[u8]; MAX_FIELDS] = [&[]; MAX_FIELDS];
    let mut field_count = 0;
    for field in line.split(|&byte| byte == b',') {
        if let Some(slot) = fields.get_mut(field_count) {
            *slot = field;
        }
        field_count += 1;
    }
    let expected_count = match fields[0] {
        b"N" | b"I" => 8,
        b"M" => 6,
        b"S" | b"A" => 4,
        b"D" => 3,
        b"C" | b"Q" => 2,
        _ => return Err(RejectCode::UnknownCommand),
    };
    if field_count != expected_count {
        return Err(RejectCode::Malformed);
    }
    let f = &fields[..field_count];
    let command = match f[0] {
        b"S" => Command::DefineSymbol {
            symbol: name(f[1])?,
            tick: integer(f[2], MAX_VALUE)?,
            lot: integer(f[3], MAX_VALUE)?,
        },
        b"N" => Command::Limit {
            order: new_order(f[1], f[2], f[3], f[5], f[7])?,
            price: integer(f[4], MAX_VALUE)?,
            tif: time_in_force(f[6])?,
        },
        b"M" => Command::Market {
            order: new_order(f[1], f[2], f[3], f[4], f[5])?,
        },
        b"I" => {
            let order = new_order(f[1], f[2], f[3], f[5], f[7])?;
            let display = integer(f[6], MAX_VALUE)?;
            if display > order.qty {
                return Err(RejectCode::Malformed);
            }
            Command::Iceberg {
                order,
                price: integer(f[4], MAX_VALUE)?,
                display,
            }
        }
        b"C" => Command::Cancel {
            id: integer(f[1], MAX_ID)?,
        },
        b"A" => Command::Amend {
            id: integer(f[1], MAX_ID)?,
            price: integer(f[2], MAX_VALUE)?,
            qty: integer(f[3], MAX_VALUE)?,
        },
        b"Q" => Command::Top {
            symbol: name(f[1])?,
        },
        b"D" => Command::Depth {
            symbol: name(f[1])?,
            levels: integer(f[2], MAX_VALUE)?,
        },
        _ => unreachable!("the command code was checked above"),
    };
    Ok(command)
}

fn new_order(
    id: &[u8],
    symbol: &[u8],
    side: &[u8],
    qty: &[u8],
    owner: &[u8],
) -> Result<NewOrder, RejectCode> {
    Ok(NewOrder {
        id: integer(id, MAX_ID)?,
        symbol: name(symbol)?,
        side: match side {
            b"B" => Side::Buy,
            b"S" => Side::Sell,
            _ => return Err(RejectCode::Malformed),
        },
        qty: integer(qty, MAX_VALUE)?,
        owner: name(owner)?,
    })
}

/// Parses 1 to 18 ASCII digits whose value is between 1 and `max`.
fn integer(field: &[u8], max: u64) -> Result<u64, RejectCode> {
    if field.is_empty() || field.len() > MAX_DIGITS || !field.iter().all(u8::is_ascii_digit) {
        return Err(RejectCode::Malformed);
    }
    let value = field
        .iter()
        .fold(0, |value, digit| value * 10 + u64::from(digit - b'0'));
    if (1..=max).contains(&value) {
        Ok(value)
    } else {
        Err(RejectCode::Malformed)
    }
}

fn name(field: &[u8]) -> Result<Name, RejectCode> {
    Name::parse(field).ok_or(RejectCode::Malformed)
}

fn time_in_force(field: &[u8]) -> Result<TimeInForce, RejectCode> {
    match field {
        b"GTC" => Ok(TimeInForce::GoodTillCancel),
        b"IOC" => Ok(TimeInForce::ImmediateOrCancel),
        b"FOK" => Ok(TimeInForce::FillOrKill),
        b"POST" => Ok(TimeInForce::PostOnly),
        _ => Err(RejectCode::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_allow_leading_zeros_up_to_eighteen_digits() {
        assert_eq!(integer(b"007", MAX_VALUE), Ok(7));
        assert_eq!(integer(b"000000000000000010", MAX_VALUE), Ok(10));
        assert_eq!(
            integer(b"0000000000000000010", MAX_VALUE),
            Err(RejectCode::Malformed)
        );
        assert_eq!(integer(b"999999999999999999", MAX_ID), Ok(MAX_ID));
    }

    #[test]
    fn integers_outside_their_range_are_malformed() {
        for field in [&b"0"[..], b"1000000001", b"+5", b"5 ", b"", b"1.0"] {
            assert_eq!(integer(field, MAX_VALUE), Err(RejectCode::Malformed));
        }
        assert_eq!(integer(b"1000000000", MAX_VALUE), Ok(MAX_VALUE));
    }

    #[test]
    fn the_command_code_is_checked_before_the_field_count() {
        assert_eq!(
            parse(b"X,1,2,3,4,5,6,7,8,9"),
            Err(RejectCode::UnknownCommand)
        );
        assert_eq!(parse(b"Q,A,B,C,D,E,F,G,H,I"), Err(RejectCode::Malformed));
        assert_eq!(parse(b""), Err(RejectCode::UnknownCommand));
    }

    #[test]
    fn the_line_length_is_checked_first() {
        let mut line = b"X,".to_vec();
        line.resize(257, b'A');
        assert_eq!(parse(&line), Err(RejectCode::Malformed));
        line.truncate(256);
        assert_eq!(parse(&line), Err(RejectCode::UnknownCommand));
    }

    #[test]
    fn display_greater_than_quantity_is_malformed() {
        assert_eq!(parse(b"I,2,ACME,B,5,10,20,AL"), Err(RejectCode::Malformed));
        assert!(parse(b"I,2,ACME,B,5,10,10,AL").is_ok());
    }
}
