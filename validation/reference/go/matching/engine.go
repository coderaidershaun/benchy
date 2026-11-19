// Package matching is a deterministic limit order book engine for several symbols.
// It turns protocol lines into protocol events and does no input or output itself.
package matching

import "strconv"

// Reason is a rejection code of the protocol.
type Reason string

const (
	UnknownCommand   Reason = "UNKNOWN_COMMAND"
	Malformed        Reason = "MALFORMED"
	UnknownSymbol    Reason = "UNKNOWN_SYMBOL"
	DuplicateSymbol  Reason = "DUPLICATE_SYMBOL"
	DuplicateOrderID Reason = "DUPLICATE_ORDER_ID"
	UnknownOrder     Reason = "UNKNOWN_ORDER"
	InvalidPrice     Reason = "INVALID_PRICE"
	InvalidQty       Reason = "INVALID_QTY"
	WouldTrade       Reason = "WOULD_TRADE"
)

// Engine holds every book of one session. The zero value is not ready; use New.
type Engine struct {
	books  map[string]*book
	live   map[int64]*order
	used   map[int64]struct{}
	lineNo int64
	out    []byte
}

// New returns an engine with no symbols.
func New() *Engine {
	return &Engine{books: map[string]*book{}, live: map[int64]*order{}, used: map[int64]struct{}{}}
}

// Apply handles the next input line, without its LF or final CR, and appends
// the events it writes to dst.
func (e *Engine) Apply(line []byte, dst []byte) []byte {
	e.lineNo++
	e.out = dst
	if reason := e.handle(line); reason != "" {
		e.out = dst
		e.event("REJECT").int(e.lineNo).text(string(reason)).end()
	}
	return e.out
}

func (e *Engine) handle(line []byte) Reason {
	if len(line) > maxLineLen {
		return Malformed
	}
	fs := splitFields(line)
	want, ok := fieldCounts[string(fs.f[0])]
	if !ok {
		return UnknownCommand
	}
	if fs.n != want {
		return Malformed
	}
	p := fieldParser{fs: fs, ok: true}
	switch fs.f[0][0] {
	case 'S':
		return e.defineSymbol(&p)
	case 'N':
		return e.newLimit(&p)
	case 'M':
		return e.newMarket(&p)
	case 'I':
		return e.newIceberg(&p)
	case 'C':
		return e.cancel(&p)
	case 'A':
		return e.amend(&p)
	case 'Q':
		return e.top(&p)
	case 'D':
		return e.depth(&p)
	}
	panic("command code in fieldCounts has no handler: " + string(fs.f[0]))
}

func (e *Engine) defineSymbol(p *fieldParser) Reason {
	name, tick, lot := p.name(), p.int(maxValue), p.int(maxValue)
	if !p.ok {
		return Malformed
	}
	if _, ok := e.books[name]; ok {
		return DuplicateSymbol
	}
	e.books[name] = newBook(name, tick, lot)
	e.event("SYMBOL").text(name).end()
	return ""
}

// orderBook runs the checks shared by every new order and gives its book.
func (e *Engine) orderBook(id int64, symbol string) (*book, Reason) {
	if _, ok := e.used[id]; ok {
		return nil, DuplicateOrderID
	}
	b, ok := e.books[symbol]
	if !ok {
		return nil, UnknownSymbol
	}
	return b, ""
}

func (e *Engine) newLimit(p *fieldParser) Reason {
	id, symbol, side, price, qty, tif, owner := p.int(maxID), p.name(), p.side(), p.int(maxValue), p.int(maxValue), p.tif(), p.name()
	if !p.ok {
		return Malformed
	}
	b, reason := e.orderBook(id, symbol)
	switch {
	case reason != "":
		return reason
	case price%b.tick != 0:
		return InvalidPrice
	case qty%b.lot != 0:
		return InvalidQty
	case tif == Post && b.wouldTrade(side, price):
		return WouldTrade
	}
	o := e.accept(b, id, owner, side, price, qty, 0)
	switch tif {
	case GTC, Post:
		e.match(o, true)
		e.rest(o)
	case IOC:
		e.match(o, true)
		e.unfilled(o)
	case FOK:
		if e.fillable(o) {
			e.match(o, true)
		}
		e.unfilled(o)
	}
	return ""
}

func (e *Engine) newMarket(p *fieldParser) Reason {
	id, symbol, side, qty, owner := p.int(maxID), p.name(), p.side(), p.int(maxValue), p.name()
	if !p.ok {
		return Malformed
	}
	b, reason := e.orderBook(id, symbol)
	switch {
	case reason != "":
		return reason
	case qty%b.lot != 0:
		return InvalidQty
	}
	o := e.accept(b, id, owner, side, 0, qty, 0)
	e.match(o, false)
	e.unfilled(o)
	return ""
}

func (e *Engine) newIceberg(p *fieldParser) Reason {
	id, symbol, side, price, qty, display, owner := p.int(maxID), p.name(), p.side(), p.int(maxValue), p.int(maxValue), p.int(maxValue), p.name()
	if !p.ok || display > qty {
		return Malformed
	}
	b, reason := e.orderBook(id, symbol)
	switch {
	case reason != "":
		return reason
	case price%b.tick != 0:
		return InvalidPrice
	case qty%b.lot != 0, display%b.lot != 0:
		return InvalidQty
	}
	o := e.accept(b, id, owner, side, price, qty, display)
	e.match(o, true)
	e.rest(o)
	return ""
}

func (e *Engine) accept(b *book, id int64, owner string, side Side, price, qty, peak int64) *order {
	e.used[id] = struct{}{}
	e.event("ACCEPT").int(id).end()
	return &order{id: id, owner: owner, side: side, price: price, open: qty, peak: peak, book: b}
}

// match trades the incoming order against the opposite side until it is filled
// or nothing it can trade with is left. A market order is not priced.
func (e *Engine) match(in *order, priced bool) {
	opposite := in.book.side(in.side.opposite())
	for in.open > 0 {
		l := opposite.bestLevel()
		if l == nil || priced && !crosses(in.side, in.price, l.price) {
			return
		}
		r := l.head
		if r.owner == in.owner {
			e.removeLive(r)
			e.event("OUT").int(r.id).int(r.open).text("SELF_TRADE").end()
			continue
		}
		q := min(in.open, r.shown)
		e.event("TRADE").text(in.book.name).int(r.price).int(q).int(in.id).int(r.id).end()
		in.open -= q
		r.open -= q
		r.shown -= q
		l.shown -= q
		switch {
		case r.open == 0:
			e.removeLive(r)
		case r.shown == 0:
			l.unlink(r)
			r.shown = r.displayFor(r.open)
			l.pushBack(r)
		}
	}
}

// fillable reports whether resting orders of other owners within the limit
// price hold enough open quantity, hidden parts included, to fill o in full.
func (e *Engine) fillable(o *order) bool {
	var sum int64
	for price, l := range o.book.side(o.side.opposite()).levels {
		if !crosses(o.side, o.price, price) {
			continue
		}
		for r := l.head; r != nil; r = r.next {
			if r.owner != o.owner {
				sum += r.open
				if sum >= o.open {
					return true
				}
			}
		}
	}
	return false
}

func (e *Engine) rest(o *order) {
	if o.open == 0 {
		return
	}
	o.shown = o.displayFor(o.open)
	o.book.side(o.side).add(o)
	e.live[o.id] = o
}

func (e *Engine) unfilled(o *order) {
	if o.open > 0 {
		e.event("OUT").int(o.id).int(o.open).text("UNFILLED").end()
	}
}

func (e *Engine) removeLive(o *order) {
	o.book.side(o.side).remove(o)
	delete(e.live, o.id)
}

func (e *Engine) cancel(p *fieldParser) Reason {
	id := p.int(maxID)
	if !p.ok {
		return Malformed
	}
	o, ok := e.live[id]
	if !ok {
		return UnknownOrder
	}
	e.removeLive(o)
	e.event("OUT").int(o.id).int(o.open).text("CANCELLED").end()
	return ""
}

func (e *Engine) amend(p *fieldParser) Reason {
	id, price, qty := p.int(maxID), p.int(maxValue), p.int(maxValue)
	if !p.ok {
		return Malformed
	}
	o, ok := e.live[id]
	switch {
	case !ok:
		return UnknownOrder
	case price%o.book.tick != 0:
		return InvalidPrice
	case qty%o.book.lot != 0:
		return InvalidQty
	case o.book.wouldTrade(o.side, price):
		return WouldTrade
	}
	if price == o.price && qty <= o.open {
		shown := min(o.shown, qty)
		o.level.shown += shown - o.shown
		o.shown, o.open = shown, qty
	} else {
		side := o.book.side(o.side)
		side.remove(o)
		o.price, o.open, o.shown = price, qty, o.displayFor(qty)
		side.add(o)
	}
	e.event("AMEND").int(id).int(price).int(qty).end()
	return ""
}

func (e *Engine) top(p *fieldParser) Reason {
	symbol := p.name()
	if !p.ok {
		return Malformed
	}
	b, ok := e.books[symbol]
	if !ok {
		return UnknownSymbol
	}
	ev := e.event("TOP").text(symbol)
	for _, l := range []*level{b.bids.bestLevel(), b.asks.bestLevel()} {
		if l == nil {
			ev = ev.int(0).int(0)
		} else {
			ev = ev.int(l.price).int(l.shown)
		}
	}
	ev.end()
	return ""
}

func (e *Engine) depth(p *fieldParser) Reason {
	symbol, n := p.name(), p.int(maxValue)
	if !p.ok {
		return Malformed
	}
	b, ok := e.books[symbol]
	if !ok {
		return UnknownSymbol
	}
	bids, asks := b.bids.sortedLevels(n), b.asks.sortedLevels(n)
	e.event("DEPTH").text(symbol).int(int64(len(bids))).int(int64(len(asks))).end()
	for _, l := range bids {
		e.event("LEVEL").text("B").int(l.price).int(l.shown).int(int64(l.count)).end()
	}
	for _, l := range asks {
		e.event("LEVEL").text("S").int(l.price).int(l.shown).int(int64(l.count)).end()
	}
	return ""
}

// eventWriter appends one comma-separated event line to the engine output.
type eventWriter struct{ e *Engine }

func (e *Engine) event(kind string) eventWriter {
	e.out = append(e.out, kind...)
	return eventWriter{e}
}

func (w eventWriter) text(s string) eventWriter {
	w.e.out = append(append(w.e.out, ','), s...)
	return w
}

func (w eventWriter) int(v int64) eventWriter {
	w.e.out = strconv.AppendInt(append(w.e.out, ','), v, 10)
	return w
}

func (w eventWriter) end() { w.e.out = append(w.e.out, '\n') }
