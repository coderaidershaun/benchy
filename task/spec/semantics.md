# Matching semantics

This document gives every rule that decides what the engine does with a command that passes its checks. The byte format of commands and events is in `protocol.md`.

Each rule has a worked example. In an example, a line that starts with `> ` is one input command, and the lines under it, up to the next `> ` line, are the events that this command writes. Every example is a whole session that starts from an empty engine, so its line numbers count from 1.

## Words

| Word | Meaning |
|---|---|
| Book | The resting orders of one symbol. It has a bid side (buy orders) and an ask side (sell orders). |
| Price level | All resting orders of one side of one book at one price. |
| Queue | The orders of one price level, in priority order. The first order in the queue trades first. |
| Incoming order | The order of the command that the engine handles now. |
| Resting order | An order that rests in a book. It is also called live. |
| Open quantity | The quantity of an order that has not traded yet. For an iceberg it is the displayed part plus the hidden part. |
| Owner | The `owner` field of an order. |

## 1. Symbols and books

Each symbol has its own book, with the tick size and lot size given by its `S` command. Orders of different symbols never trade with each other. A symbol cannot be defined twice, and an order for a symbol that is not defined is rejected.

```text
> S,ACME,5,10
SYMBOL,ACME
> S,BETA,1,1
SYMBOL,BETA
> S,ACME,1,1
REJECT,3,DUPLICATE_SYMBOL
> N,1,ACME,B,100,10,GTC,AL
ACCEPT,1
> N,2,BETA,S,100,10,GTC,BO
ACCEPT,2
> N,3,ZED,S,100,10,GTC,BO
REJECT,6,UNKNOWN_SYMBOL
> Q,ACME
TOP,ACME,100,10,0,0
> Q,BETA
TOP,BETA,0,0,100,10
```

The buy of `ACME` and the sell of `BETA` have the same price but do not trade, because they are in different books.

## 2. Tick and lot

The price of an order, and the new price of an amend, must be a whole multiple of the tick size of its symbol. The quantity, the display quantity of an iceberg and the new quantity of an amend must be a whole multiple of the lot size. A market order has no price, so only its quantity is checked.

```text
> S,ACME,5,10
SYMBOL,ACME
> N,1,ACME,B,102,10,GTC,AL
REJECT,2,INVALID_PRICE
> N,1,ACME,B,100,15,GTC,AL
REJECT,3,INVALID_QTY
> N,1,ACME,B,100,20,GTC,AL
ACCEPT,1
> N,1,ACME,B,100,20,GTC,AL
REJECT,5,DUPLICATE_ORDER_ID
```

Lines 2 and 3 use no id, so line 4 can use id 1. After line 4, id 1 is used.

## 3. Price-time priority

An incoming buy trades first with the ask of the lowest price. An incoming sell trades first with the bid of the highest price. Within one price level, the order that joined the queue first trades first.

An order joins the back of a queue when it starts to rest, when an amend makes it lose priority (rule 13), and when an iceberg refills its displayed part (rule 11).

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,101,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,100,10,GTC,BO
ACCEPT,2
> N,3,ACME,S,100,10,GTC,CY
ACCEPT,3
> N,4,ACME,B,101,25,GTC,DI
ACCEPT,4
TRADE,ACME,100,10,4,2
TRADE,ACME,100,10,4,3
TRADE,ACME,101,5,4,1
```

Order 2 and order 3 have the better price, and order 2 came first. Order 1 keeps 5 open.

## 4. Matching and the trade price

An incoming buy with price `P` can trade with an ask at price `P` or lower. An incoming sell with price `P` can trade with a bid at price `P` or higher. A market order can trade at any price.

The engine matches in steps. Each step takes the first order in the queue of the best opposite price level that the incoming order can trade with, and trades the smaller of the two quantities: the open quantity of the incoming order, and the displayed quantity of the resting order. The trade price is always the price of the resting order. Matching stops when the incoming order has no open quantity left, or when no opposite order is left that it can trade with.

A resting order whose open quantity reaches 0 leaves the book and writes no event of its own.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,98,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,99,10,GTC,BO
ACCEPT,2
> N,3,ACME,B,100,15,GTC,CY
ACCEPT,3
TRADE,ACME,98,10,3,1
TRADE,ACME,99,5,3,2
> Q,ACME
TOP,ACME,0,0,99,5
```

The buyer offered 100, but each trade takes the price of the resting order.

Because every incoming order matches before it rests, and post-only orders and amends that would trade are rejected, the book is never crossed: after every command, the best bid is lower than the best ask.

## 5. Limit orders that rest (`GTC`)

A limit order with `tif` `GTC` matches, and then any open quantity rests in the book at the order's price.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,100,25,GTC,BO
ACCEPT,2
TRADE,ACME,100,10,2,1
> Q,ACME
TOP,ACME,100,15,0,0
```

## 6. Market orders (`M`)

A market order matches at any price until it has no open quantity left or the opposite side is empty. It never rests. Any open quantity left is removed with `OUT` and reason `UNFILLED`.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,105,10,GTC,AL
ACCEPT,2
> M,3,ACME,B,25,BO
ACCEPT,3
TRADE,ACME,100,10,3,1
TRADE,ACME,105,10,3,2
OUT,3,5,UNFILLED
> M,4,ACME,S,10,BO
ACCEPT,4
OUT,4,10,UNFILLED
```

Order 4 finds no bid at all, so all of it is removed.

## 7. Immediate-or-cancel (`IOC`)

A limit order with `tif` `IOC` matches like any limit order, and never rests. Any open quantity left is removed with `OUT` and reason `UNFILLED`.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,105,10,GTC,AL
ACCEPT,2
> N,3,ACME,B,100,25,IOC,BO
ACCEPT,3
TRADE,ACME,100,10,3,1
OUT,3,15,UNFILLED
```

Order 2 is above the limit of order 3, so it does not trade.

## 8. Fill-or-kill (`FOK`)

A limit order with `tif` `FOK` trades its whole quantity at once, or not at all. Before it matches, the engine adds up the open quantity, hidden parts included, of every opposite resting order that it could trade with by price and whose owner differs from its own. If that sum is less than the order's quantity, the order writes `ACCEPT` and then `OUT` for its whole quantity with reason `UNFILLED`, and nothing else happens: no trade, and no self-trade removal. If the sum is enough, the order matches like an `IOC` order, and it is then always filled in full.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,101,10,GTC,AL
ACCEPT,2
> N,3,ACME,B,101,25,FOK,BO
ACCEPT,3
OUT,3,25,UNFILLED
> N,4,ACME,B,101,20,FOK,BO
ACCEPT,4
TRADE,ACME,100,10,4,1
TRADE,ACME,101,10,4,2
```

## 9. Post-only (`POST`)

A limit order with `tif` `POST` must not trade when it arrives. If it would trade with any opposite resting order, whatever that order's owner, the engine rejects it with `WOULD_TRADE`. It would trade when the opposite side is not empty and its best price is one the order can trade with (rule 4). Otherwise it is accepted and rests like a `GTC` order.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,100,10,POST,AL
REJECT,3,WOULD_TRADE
> N,2,ACME,B,99,10,POST,AL
ACCEPT,2
> Q,ACME
TOP,ACME,99,10,100,10
```

Line 3 is rejected even though the only order it would meet has the same owner.

## 10. Self-trade prevention

Two orders of the same owner never trade with each other. The policy is **cancel resting**: when the next step of matching would trade an incoming order with a resting order of the same owner, the engine instead removes the resting order with `OUT`, its whole open quantity and reason `SELF_TRADE`. Matching then goes on with the next resting order, as if the removed order had never been there. The incoming order loses no quantity.

The rule applies to every incoming order that matches: `GTC`, `IOC` and `FOK` limit orders, market orders and icebergs. A post-only order and an amend never match, so it does not apply to them.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,100,10,GTC,BO
ACCEPT,2
> N,3,ACME,S,101,10,GTC,AL
ACCEPT,3
> N,4,ACME,B,101,15,GTC,AL
ACCEPT,4
OUT,1,10,SELF_TRADE
TRADE,ACME,100,10,4,2
OUT,3,10,SELF_TRADE
> Q,ACME
TOP,ACME,101,5,0,0
```

Order 4 removes order 1, trades with order 2, removes order 3, and rests with 5 open.

With fill-or-kill, the resting orders of the same owner do not count toward the sum of rule 8. When the order does fill, it removes each order of its owner that it meets on the way, and only those.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,100,10,GTC,BO
ACCEPT,2
> N,3,ACME,S,100,10,GTC,AL
ACCEPT,3
> N,4,ACME,B,100,15,FOK,AL
ACCEPT,4
OUT,4,15,UNFILLED
> N,5,ACME,B,100,10,FOK,AL
ACCEPT,5
OUT,1,10,SELF_TRADE
TRADE,ACME,100,10,5,2
> Q,ACME
TOP,ACME,0,0,100,10
```

Order 4 could trade only with order 2, which has 10 open, so it is not filled and removes nothing. Order 5 is filled by order 2, so it removes order 1 on the way but never reaches order 3.

## 11. Iceberg orders (`I`)

An iceberg is a `GTC` limit order that shows only part of its open quantity. Its `display` field is its peak size.

- When it arrives, it matches with its whole quantity, like any `GTC` order.
- When it rests, its displayed part is the smaller of the peak size and its open quantity. The rest is hidden.
- As a resting order, each step of matching trades at most its displayed part.
- When its displayed part reaches 0 and a hidden part is left, it refills at once: the displayed part becomes the smaller of the peak size and the hidden part, which shrinks by the same amount. The order then moves to the back of the queue at its price. This happens in the middle of matching, so the same incoming order can trade with it again after the other orders at that price. It also happens when the trade that emptied the displayed part filled the incoming order, so the iceberg is behind the other orders at its price before the next command.
- `TOP` and `LEVEL` events count only displayed parts. Fill-or-kill counts hidden parts too (rule 8).
- When an iceberg is cancelled or removed by self-trade prevention, the `OUT` event carries its whole open quantity, hidden part included.

```text
> S,ACME,1,1
SYMBOL,ACME
> I,1,ACME,S,100,30,10,AL
ACCEPT,1
> N,2,ACME,S,100,5,GTC,BO
ACCEPT,2
> Q,ACME
TOP,ACME,0,0,100,15
> N,3,ACME,B,100,22,GTC,CY
ACCEPT,3
TRADE,ACME,100,10,3,1
TRADE,ACME,100,5,3,2
TRADE,ACME,100,7,3,1
> D,ACME,5
DEPTH,ACME,0,1
LEVEL,S,100,3,1
> C,1
OUT,1,13,CANCELLED
```

Order 3 takes the 10 shown by order 1. Order 1 refills to 10 and moves behind order 2, so order 3 trades next with order 2, and then takes 7 more from order 1. Order 1 then shows 3 and hides 10.

An incoming iceberg trades its whole quantity before it rests.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> I,2,ACME,B,100,50,10,BO
ACCEPT,2
TRADE,ACME,100,10,2,1
> Q,ACME
TOP,ACME,100,10,0,0
> N,3,ACME,S,100,25,GTC,CY
ACCEPT,3
TRADE,ACME,100,10,3,2
TRADE,ACME,100,10,3,2
TRADE,ACME,100,5,3,2
> Q,ACME
TOP,ACME,100,5,0,0
```

Order 2 rests with 40 open: 10 shown and 30 hidden. Order 3 meets it alone at its price, so each refill trades again with the same order. Order 2 then shows 5 and hides 10.

An iceberg that refills moves back even when the incoming order is already filled:

```text
> S,ACME,1,1
SYMBOL,ACME
> I,1,ACME,S,100,20,10,AL
ACCEPT,1
> N,2,ACME,S,100,5,GTC,BO
ACCEPT,2
> N,3,ACME,B,100,10,GTC,CY
ACCEPT,3
TRADE,ACME,100,10,3,1
> N,4,ACME,B,100,5,GTC,DI
ACCEPT,4
TRADE,ACME,100,5,4,2
> Q,ACME
TOP,ACME,0,0,100,10
```

Order 3 empties the 10 that order 1 shows and is filled. Order 1 refills and moves behind order 2, so order 4 trades with order 2.

## 12. Cancel (`C`)

A cancel removes a live order and writes `OUT` with its whole open quantity and reason `CANCELLED`. An id that is not live (never used, filled, cancelled, removed, or of an order that never rested) is rejected with `UNKNOWN_ORDER`.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,S,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,100,4,GTC,BO
ACCEPT,2
TRADE,ACME,100,4,2,1
> C,1
OUT,1,6,CANCELLED
> C,1
REJECT,5,UNKNOWN_ORDER
> C,2
REJECT,6,UNKNOWN_ORDER
```

## 13. Amend (`A`)

An amend sets a new price and a new open quantity for a live order. Side, owner, symbol, id and, for an iceberg, peak size do not change. To remove an order, use cancel: a quantity of 0 is `MALFORMED`.

- If the new price would let the order trade with the opposite side (rule 9 states when), the amend is rejected with `WOULD_TRADE` and the order does not change. An amend never trades.
- **Keeps priority**: when the price is the same and the new quantity is not greater than the open quantity, the order keeps its place in the queue. For an iceberg, the displayed part becomes the smaller of its current displayed part and the new quantity, and the hidden part is the rest.
- **Loses priority**: when the price changes, or the new quantity is greater than the open quantity, the order moves to the back of the queue at the new price. For an iceberg, the displayed part becomes the smaller of the peak size and the new quantity, and the hidden part is the rest.
- In both cases the engine writes `AMEND` with the new price and the new quantity, also when nothing changed.

Reducing the quantity keeps priority:

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,B,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,100,10,GTC,BO
ACCEPT,2
> A,1,100,5
AMEND,1,100,5
> N,3,ACME,S,100,5,GTC,CY
ACCEPT,3
TRADE,ACME,100,5,3,1
```

Raising the quantity loses priority, and so does a change of price:

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,B,100,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,100,10,GTC,BO
ACCEPT,2
> A,1,100,15
AMEND,1,100,15
> N,3,ACME,S,100,5,GTC,CY
ACCEPT,3
TRADE,ACME,100,5,3,2
> A,2,99,5
AMEND,2,99,5
> D,ACME,5
DEPTH,ACME,2,0
LEVEL,B,100,15,1
LEVEL,B,99,5,1
```

An amend that would trade is rejected:

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,B,99,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,100,10,GTC,BO
ACCEPT,2
> A,1,100,10
REJECT,4,WOULD_TRADE
> A,1,98,10
AMEND,1,98,10
> A,3,98,10
REJECT,6,UNKNOWN_ORDER
```

An iceberg keeps its peak size through amends:

```text
> S,ACME,1,1
SYMBOL,ACME
> I,1,ACME,S,100,30,10,AL
ACCEPT,1
> N,2,ACME,S,100,5,GTC,BO
ACCEPT,2
> A,1,100,25
AMEND,1,100,25
> A,1,100,8
AMEND,1,100,8
> Q,ACME
TOP,ACME,0,0,100,13
> A,1,100,40
AMEND,1,100,40
> N,3,ACME,B,100,7,GTC,CY
ACCEPT,3
TRADE,ACME,100,5,3,2
TRADE,ACME,100,2,3,1
> Q,ACME
TOP,ACME,0,0,100,8
```

Order 1 first shows 10 and hides 20. The amend to 25 keeps its place and hides 15. The amend to 8 shows 8 and hides nothing. The amend to 40 raises the quantity, so order 1 moves behind order 2 and shows 10 again, hiding 30. Order 3 then trades with order 2 first.

## 14. Top of book (`Q`)

`TOP` gives, for each side, the best price and the sum of the displayed quantities of all orders at that price. An empty side is written `0,0`.

```text
> S,ACME,1,1
SYMBOL,ACME
> Q,ACME
TOP,ACME,0,0,0,0
> N,1,ACME,B,99,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,99,5,GTC,BO
ACCEPT,2
> N,3,ACME,B,98,7,GTC,CY
ACCEPT,3
> Q,ACME
TOP,ACME,99,15,0,0
```

## 15. Depth (`D`)

`DEPTH` gives the number of bid levels and ask levels that follow, each at most `levels`. Then come the bid levels from the highest price down, then the ask levels from the lowest price up. Each `LEVEL` gives the sum of the displayed quantities at that price and the number of orders at that price.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,B,99,10,GTC,AL
ACCEPT,1
> N,2,ACME,B,99,5,GTC,BO
ACCEPT,2
> N,3,ACME,B,98,7,GTC,CY
ACCEPT,3
> N,4,ACME,S,101,4,GTC,DI
ACCEPT,4
> I,5,ACME,S,102,50,6,EV
ACCEPT,5
> D,ACME,1
DEPTH,ACME,1,1
LEVEL,B,99,15,2
LEVEL,S,101,4,1
> D,ACME,10
DEPTH,ACME,2,2
LEVEL,B,99,15,2
LEVEL,B,98,7,1
LEVEL,S,101,4,1
LEVEL,S,102,6,1
```

## 16. Rejections change nothing

A rejected command writes one `REJECT` event and leaves the engine exactly as it was: no order, no id use, no symbol, no change to any book. The protocol fixes the order of the checks, so one line always gets one code.

```text
> S,ACME,1,1
SYMBOL,ACME
> N,1,ACME,B,99,10,GTC,AL
ACCEPT,1
> N,2,ACME,S,99,10,POST,BO
REJECT,3,WOULD_TRADE
> C,2
REJECT,4,UNKNOWN_ORDER
> N,2,ACME,S,100,10,GTC,BO
ACCEPT,2
> Q,ACME
TOP,ACME,99,10,100,10
```
