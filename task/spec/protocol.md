# Engine protocol

This document fixes every byte that the engine program reads and writes. The matching rules that decide which events a command produces are in `semantics.md`. Where the two seem to disagree, tell us in `DELIVERY.md` and follow this document for formats and `semantics.md` for behaviour.

## 1. The program

- The program reads commands from standard input until end of input and writes events to standard output.
- It takes no arguments. With the single argument `--help` it writes a short usage text to standard output and exits with code 0. With any other arguments it writes the usage text to standard error and exits with code 2.
- When input ends, it writes any output it still holds, flushes standard output and exits with code 0. Rejected commands do not change the exit code.
- Output may be buffered. The only requirement is that all of it is written and flushed before the program exits. Nothing reads the output while the program runs.
- Standard error is not checked. A rejected command is reported on standard output, never on standard error.
- The program is single-threaded in behaviour: the output depends only on the input bytes. It reads no clock, no random source, no environment variable and no file.
- One session holds at most 10,000,000 lines.

## 2. Input lines

- Input is a sequence of bytes, divided into lines at each LF byte (`0x0A`). The LF is not part of the line.
- If the input does not end with LF, the bytes after the last LF are the final line. Input that ends with LF has no empty line after it.
- If a line ends with a CR byte (`0x0D`), that one CR is removed. This also applies to the final line when it has no LF. Any other CR is an ordinary byte.
- Lines are numbered from 1. Every line counts, including empty and rejected lines. The line number appears in `REJECT` events.
- Input is not decoded as text. Bytes outside ASCII are ordinary bytes, and they always fail the field rules below.

| Input bytes | Lines |
|---|---|
| `Q,ACME` LF `Q,ACME` LF | line 1 `Q,ACME`, line 2 `Q,ACME` |
| `Q,ACME` CR LF `Q,ACME` | line 1 `Q,ACME`, line 2 `Q,ACME` |
| `Q,ACME` CR CR LF | line 1 `Q,ACME` CR, which is rejected `MALFORMED` because CR is not allowed in a name |
| LF LF `Q,ACME` LF | line 1 empty, line 2 empty, line 3 `Q,ACME` |

## 3. Parsing a line

The engine handles each line by these steps, in this order. The first step that fails decides the rejection code, and the engine writes exactly one `REJECT` event for that line and changes nothing else.

1. If the line is longer than 256 bytes (counted after the CR is removed), reject it `MALFORMED`.
2. Divide the line into fields at each comma. Fields are not trimmed: a space is part of its field. A line with no comma is one field, and an empty line is one empty field.
3. If the first field is not exactly one of the command codes in section 5, reject it `UNKNOWN_COMMAND`. Codes are case-sensitive.
4. If the number of fields is not the number that the command takes, reject it `MALFORMED`.
5. If any field does not satisfy its field type in section 4, reject it `MALFORMED`.
6. Run the checks of the command, in the order section 5 lists them. The first check that fails gives the code.

Every failure in steps 1 to 5 can be decided from the line alone. Every code that step 6 gives depends on the state of the engine.

Worked examples. Each line is line number 7 of its session. In the last two rows, order 1 is already used, symbol `ACME` is defined with tick 5 and lot 10, and symbol `NOPE` is not defined.

| Line | Event | Why |
|---|---|---|
| (empty) | `REJECT,7,UNKNOWN_COMMAND` | The first field is empty. |
| `q,ACME` | `REJECT,7,UNKNOWN_COMMAND` | Codes are case-sensitive. |
| `X,1` | `REJECT,7,UNKNOWN_COMMAND` | `X` is no command. |
| `Q,ACME,1` | `REJECT,7,MALFORMED` | `Q` takes 2 fields, not 3. |
| `Q,ACME,` | `REJECT,7,MALFORMED` | The trailing comma makes a third, empty field. |
| `Q, ACME` | `REJECT,7,MALFORMED` | ` ACME` starts with a space, so it is not a name. |
| `Q,acme` | `REJECT,7,MALFORMED` | Names are uppercase. |
| `Q,ABCDEFGHI` | `REJECT,7,MALFORMED` | A name has at most 8 bytes. |
| `N,1,ACME,B,0,10,GTC,AL` | `REJECT,7,MALFORMED` | 0 is outside the range of `price`. |
| `N,1,ACME,B,+5,10,GTC,AL` | `REJECT,7,MALFORMED` | A sign is not a digit. |
| `N,0,ACME,B,5,10,GTC,AL` | `REJECT,7,MALFORMED` | 0 is outside the range of `id`. |
| `N,2,ACME,B,5,10,DAY,AL` | `REJECT,7,MALFORMED` | `DAY` is not a `tif`. |
| `I,2,ACME,B,5,10,20,AL` | `REJECT,7,MALFORMED` | `display` 20 is greater than `qty` 10. |
| `Q,` followed by 300 `A` bytes | `REJECT,7,MALFORMED` | The line is longer than 256 bytes, which is checked before anything else. |
| `N,1,NOPE,B,3,15,GTC,AL` | `REJECT,7,DUPLICATE_ORDER_ID` | Every check fails, and `id` is checked first. |
| `N,2,ACME,B,3,15,GTC,AL` | `REJECT,7,INVALID_PRICE` | The price is checked before the quantity. |

## 4. Field types

| Type | Rule | Range |
|---|---|---|
| `id` | integer | 1 to 999,999,999,999,999,999 |
| `price` | integer | 1 to 1,000,000,000 |
| `qty` | integer | 1 to 1,000,000,000 |
| `display` | integer, and not greater than the `qty` field of the same line | 1 to 1,000,000,000 |
| `tick` | integer | 1 to 1,000,000,000 |
| `lot` | integer | 1 to 1,000,000,000 |
| `levels` | integer | 1 to 1,000,000,000 |
| `symbol` | name | |
| `owner` | name | |
| `side` | exactly `B` (buy) or `S` (sell) | |
| `tif` | exactly `GTC`, `IOC`, `FOK` or `POST` | |

- An **integer** is 1 to 18 ASCII digits (`0` to `9`) and nothing else: no sign, no space, no decimal point. Leading zeros are allowed, so `007` is 7 and `0000000000000000010` (19 digits) fails. A value outside the range of its type fails, so `0` fails every integer type.
- A **name** is 1 to 8 bytes, each an uppercase ASCII letter (`A` to `Z`) or an ASCII digit.
- Because every number fits in a signed 64-bit integer, and a session holds at most 10,000,000 lines, every sum of quantities that the engine computes also fits in a signed 64-bit integer.

## 5. Commands

| Code | Fields, in order | Does |
|---|---|---|
| `S` | `S,symbol,tick,lot` | Define a symbol with its tick size and lot size. |
| `N` | `N,id,symbol,side,price,qty,tif,owner` | Enter a limit order with a time in force. |
| `M` | `M,id,symbol,side,qty,owner` | Enter a market order. |
| `I` | `I,id,symbol,side,price,qty,display,owner` | Enter an iceberg limit order (good till cancelled). |
| `C` | `C,id` | Cancel a live order. |
| `A` | `A,id,price,qty` | Amend the price and open quantity of a live order. |
| `Q` | `Q,symbol` | Ask for the top of book. |
| `D` | `D,symbol,levels` | Ask for book depth, up to `levels` price levels on each side. |

Checks of step 6, in order. A check that names a field runs on that field's value.

| Code | Checks, first to last |
|---|---|
| `S` | `symbol` is not defined yet, else `DUPLICATE_SYMBOL`. |
| `N` | `id` is not used, else `DUPLICATE_ORDER_ID`. `symbol` is defined, else `UNKNOWN_SYMBOL`. `price` is a multiple of the tick, else `INVALID_PRICE`. `qty` is a multiple of the lot, else `INVALID_QTY`. When `tif` is `POST`: the order would not trade on entry, else `WOULD_TRADE`. |
| `M` | `id` is not used, else `DUPLICATE_ORDER_ID`. `symbol` is defined, else `UNKNOWN_SYMBOL`. `qty` is a multiple of the lot, else `INVALID_QTY`. |
| `I` | `id` is not used, else `DUPLICATE_ORDER_ID`. `symbol` is defined, else `UNKNOWN_SYMBOL`. `price` is a multiple of the tick, else `INVALID_PRICE`. `qty` is a multiple of the lot, else `INVALID_QTY`. `display` is a multiple of the lot, else `INVALID_QTY`. |
| `C` | `id` is a live order, else `UNKNOWN_ORDER`. |
| `A` | `id` is a live order, else `UNKNOWN_ORDER`. `price` is a multiple of the tick of the order's symbol, else `INVALID_PRICE`. `qty` is a multiple of the lot of the order's symbol, else `INVALID_QTY`. The order at the new price would not trade, else `WOULD_TRADE`. |
| `Q` | `symbol` is defined, else `UNKNOWN_SYMBOL`. |
| `D` | `symbol` is defined, else `UNKNOWN_SYMBOL`. |

- An `id` is **used** from the moment its order is accepted (its `ACCEPT` event) to the end of the session, also after the order is filled, cancelled or removed. A rejected command uses no id.
- A **live** order is one that rests in a book. Market, immediate-or-cancel and fill-or-kill orders are never live after their own command.
- Order ids are one namespace across all symbols.

## 6. Events

Each event is one line: fields joined by single commas, then one LF. No CR, no spaces, no empty fields. Integers are written in decimal with no sign and no leading zeros, so an input `007` is written `7`. Names are written as they were given.

| Event | Fields, in order | Meaning |
|---|---|---|
| `SYMBOL` | `SYMBOL,symbol` | The symbol is defined. |
| `ACCEPT` | `ACCEPT,id` | The order passed every check. |
| `REJECT` | `REJECT,line,code` | The command on line number `line` was rejected with `code`. Nothing changed. |
| `TRADE` | `TRADE,symbol,price,qty,taker,maker` | `qty` traded at `price`. `taker` is the id of the incoming order, `maker` the id of the resting order. |
| `OUT` | `OUT,id,qty,reason` | Order `id` left the book, or will never rest, with `qty` not traded. |
| `AMEND` | `AMEND,id,price,qty` | The amend was applied. `price` and `qty` are the new price and the new open quantity. |
| `TOP` | `TOP,symbol,bid_price,bid_qty,ask_price,ask_qty` | The best price of each side and the displayed quantity at it. An empty side is written `0,0`. |
| `DEPTH` | `DEPTH,symbol,bid_levels,ask_levels` | The number of `LEVEL` events that follow for each side. |
| `LEVEL` | `LEVEL,side,price,qty,orders` | One price level: its displayed quantity and the number of orders at it. |

`OUT` reasons:

| Reason | When |
|---|---|
| `CANCELLED` | A `C` command removed the order. |
| `UNFILLED` | The part of a market, immediate-or-cancel or fill-or-kill order that did not trade. |
| `SELF_TRADE` | Self-trade prevention removed a resting order. |

Rejection codes:

| Code | Meaning |
|---|---|
| `UNKNOWN_COMMAND` | The first field is not a command code. |
| `MALFORMED` | The line is too long, has the wrong number of fields, or a field breaks its type. |
| `UNKNOWN_SYMBOL` | The symbol is not defined. |
| `DUPLICATE_SYMBOL` | The symbol is already defined. |
| `DUPLICATE_ORDER_ID` | The order id is already used. |
| `UNKNOWN_ORDER` | No live order has this id. |
| `INVALID_PRICE` | The price is not a multiple of the symbol's tick. |
| `INVALID_QTY` | The quantity or display quantity is not a multiple of the symbol's lot. |
| `WOULD_TRADE` | A post-only order, or an amend, would trade at once. |

## 7. Events of one command

A command produces its events in this order, before the engine reads the next line.

| Command | Events |
|---|---|
| Any rejected command | One `REJECT`. Nothing else. |
| `S` | `SYMBOL`. |
| `N`, `M`, `I` | `ACCEPT`. Then one event for each step of matching, in the order the steps happen: a `TRADE`, or an `OUT` with reason `SELF_TRADE` for a resting order of the same owner. Then, when quantity is left and the order does not rest, `OUT` for the incoming order with reason `UNFILLED`. An order that rests writes no further event. |
| `N` with `FOK` that cannot fill in full | `ACCEPT`, then `OUT` for its whole quantity with reason `UNFILLED`. No other event. |
| `C` | `OUT` with reason `CANCELLED` and the order's open quantity. |
| `A` | `AMEND`. |
| `Q` | `TOP`. |
| `D` | `DEPTH`, then one `LEVEL` for each bid level from the highest price down, then one `LEVEL` for each ask level from the lowest price up. |

A resting order that is filled in full writes no event of its own: the `TRADE` events show it.

## 8. Build and run

The `Makefile` builds each program to a fixed path: `bin/rust/engine`, `bin/go/engine` and `bin/csharp/Engine`. The scorer runs each of them with the same input and compares the output with the expected output byte for byte.
