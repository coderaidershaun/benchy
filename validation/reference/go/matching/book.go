// Order book of one symbol: price levels kept in a map for direct access and a heap for the best price.

package matching

import (
	"container/heap"
	"sort"
)

// Side is the side of the book an order rests on.
type Side byte

const (
	Buy  Side = 'B'
	Sell Side = 'S'
)

func (s Side) opposite() Side {
	if s == Buy {
		return Sell
	}
	return Buy
}

type order struct {
	id    int64
	owner string
	side  Side
	price int64
	open  int64 // displayed plus hidden quantity
	shown int64
	peak  int64 // 0 when the order is not an iceberg
	level *level
	prev  *order
	next  *order
	book  *book
}

// displayFor gives the displayed part of an order that refills to open quantity q.
func (o *order) displayFor(q int64) int64 {
	if o.peak == 0 {
		return q
	}
	return min(o.peak, q)
}

// level is a FIFO queue of the orders at one price, linked through the orders.
type level struct {
	price int64
	head  *order
	tail  *order
	shown int64
	count int
}

func (l *level) pushBack(o *order) {
	o.level = l
	o.prev, o.next = l.tail, nil
	if l.tail == nil {
		l.head = o
	} else {
		l.tail.next = o
	}
	l.tail = o
	l.shown += o.shown
	l.count++
}

func (l *level) unlink(o *order) {
	if o.prev == nil {
		l.head = o.next
	} else {
		o.prev.next = o.next
	}
	if o.next == nil {
		l.tail = o.prev
	} else {
		o.next.prev = o.prev
	}
	o.prev, o.next, o.level = nil, nil, nil
	l.shown -= o.shown
	l.count--
}

// priceHeap orders prices best first. It may hold prices whose level is gone;
// those are dropped lazily when they reach the top.
type priceHeap struct {
	prices []int64
	better func(a, b int64) bool
}

func (h *priceHeap) Len() int           { return len(h.prices) }
func (h *priceHeap) Less(i, j int) bool { return h.better(h.prices[i], h.prices[j]) }
func (h *priceHeap) Swap(i, j int)      { h.prices[i], h.prices[j] = h.prices[j], h.prices[i] }
func (h *priceHeap) Push(x any)         { h.prices = append(h.prices, x.(int64)) }
func (h *priceHeap) Pop() any {
	last := h.prices[len(h.prices)-1]
	h.prices = h.prices[:len(h.prices)-1]
	return last
}

type bookSide struct {
	levels map[int64]*level
	best   priceHeap
}

func newBookSide(s Side) *bookSide {
	better := func(a, b int64) bool { return a > b }
	if s == Sell {
		better = func(a, b int64) bool { return a < b }
	}
	return &bookSide{levels: map[int64]*level{}, best: priceHeap{better: better}}
}

func (bs *bookSide) bestLevel() *level {
	for bs.best.Len() > 0 {
		if l, ok := bs.levels[bs.best.prices[0]]; ok {
			return l
		}
		heap.Pop(&bs.best)
	}
	return nil
}

func (bs *bookSide) add(o *order) {
	l, ok := bs.levels[o.price]
	if !ok {
		l = &level{price: o.price}
		bs.levels[o.price] = l
		heap.Push(&bs.best, o.price)
	}
	l.pushBack(o)
}

func (bs *bookSide) remove(o *order) {
	l := o.level
	l.unlink(o)
	if l.count == 0 {
		delete(bs.levels, l.price)
	}
}

// sortedLevels gives at most n levels, best first.
func (bs *bookSide) sortedLevels(n int64) []*level {
	all := make([]*level, 0, len(bs.levels))
	for _, l := range bs.levels {
		all = append(all, l)
	}
	sort.Slice(all, func(i, j int) bool { return bs.best.better(all[i].price, all[j].price) })
	if int64(len(all)) > n {
		all = all[:n]
	}
	return all
}

type book struct {
	name string
	tick int64
	lot  int64
	bids *bookSide
	asks *bookSide
}

func newBook(name string, tick, lot int64) *book {
	return &book{name: name, tick: tick, lot: lot, bids: newBookSide(Buy), asks: newBookSide(Sell)}
}

func (b *book) side(s Side) *bookSide {
	if s == Buy {
		return b.bids
	}
	return b.asks
}

// crosses reports whether an order of side s at price p can trade with resting price rp.
func crosses(s Side, p, rp int64) bool {
	if s == Buy {
		return rp <= p
	}
	return rp >= p
}

// wouldTrade reports whether a limit order of side s at price p meets the opposite best price.
func (b *book) wouldTrade(s Side, p int64) bool {
	best := b.side(s.opposite()).bestLevel()
	return best != nil && crosses(s, p, best.price)
}
