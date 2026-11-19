// Invariant tests over random sessions: the book stays uncrossed, quantity is conserved and queues fill in arrival order.

package matching

import (
	"fmt"
	"math/rand"
	"strconv"
	"strings"
	"testing"
)

func randomCommand(r *rand.Rand, nextID int64) string {
	side := []string{"B", "S"}[r.Intn(2)]
	owner := []string{"AL", "BO", "CY"}[r.Intn(3)]
	price := 95 + r.Intn(11)
	qty := 1 + r.Intn(20)
	oldID := 1 + r.Int63n(nextID)
	switch r.Intn(10) {
	case 0:
		return fmt.Sprintf("M,%d,ACME,%s,%d,%s", nextID, side, qty, owner)
	case 1:
		return fmt.Sprintf("I,%d,ACME,%s,%d,%d,%d,%s", nextID, side, price, qty, 1+r.Intn(qty), owner)
	case 2:
		return fmt.Sprintf("C,%d", oldID)
	case 3:
		return fmt.Sprintf("A,%d,%d,%d", oldID, price, qty)
	default:
		tif := []string{"GTC", "GTC", "IOC", "FOK", "POST"}[r.Intn(5)]
		return fmt.Sprintf("N,%d,ACME,%s,%d,%d,%s,%s", nextID, side, price, qty, tif, owner)
	}
}

// ledger follows each order's quantity through the events: what it was given,
// and how much has traded or left.
type ledger struct {
	given     map[int64]int64
	accounted map[int64]int64
	reordered map[int64]bool // icebergs and amended orders change place in their queue
}

func (lg *ledger) observe(t *testing.T, cmd string, events []byte) {
	t.Helper()
	f := strings.Split(cmd, ",")
	lastMaker := map[string]int64{}
	for _, ev := range strings.Split(strings.TrimSuffix(string(events), "\n"), "\n") {
		g := strings.Split(ev, ",")
		num := func(i int) int64 { v, _ := strconv.ParseInt(g[i], 10, 64); return v }
		switch g[0] {
		case "ACCEPT":
			qty, _ := strconv.ParseInt(f[map[string]int{"N": 5, "M": 4, "I": 5}[f[0]]], 10, 64)
			lg.given[num(1)] = qty
			lg.reordered[num(1)] = f[0] == "I"
		case "TRADE":
			lg.accounted[num(4)] += num(3)
			lg.accounted[num(5)] += num(3)
			maker := num(5)
			if !lg.reordered[maker] {
				if prev, ok := lastMaker[g[2]]; ok && prev > maker {
					t.Fatalf("%s: maker %d traded after later order %d at price %s", cmd, maker, prev, g[2])
				}
				lastMaker[g[2]] = maker
			}
		case "OUT":
			lg.accounted[num(1)] += num(2)
		case "AMEND":
			lg.given[num(1)] = lg.accounted[num(1)] + num(3)
			lg.reordered[num(1)] = true
		}
	}
}

func checkBook(t *testing.T, e *Engine, cmd string) {
	t.Helper()
	b := e.books["ACME"]
	bid, ask := b.bids.bestLevel(), b.asks.bestLevel()
	if bid != nil && ask != nil && bid.price >= ask.price {
		t.Fatalf("after %s: book crossed, bid %d ask %d", cmd, bid.price, ask.price)
	}
	resting := 0
	for _, side := range []*bookSide{b.bids, b.asks} {
		for price, l := range side.levels {
			var shown int64
			count := 0
			for o := l.head; o != nil; o = o.next {
				if o.price != price || o.shown <= 0 || o.shown > o.open || e.live[o.id] != o {
					t.Fatalf("after %s: bad resting order %+v", cmd, *o)
				}
				shown += o.shown
				count++
			}
			if shown != l.shown || count != l.count || count == 0 {
				t.Fatalf("after %s: level %d holds %d shown in %d orders, records %d in %d", cmd, price, shown, count, l.shown, l.count)
			}
			resting += count
		}
	}
	if resting != len(e.live) {
		t.Fatalf("after %s: %d orders rest but %d are live", cmd, resting, len(e.live))
	}
}

func TestRandomSessionsKeepInvariants(t *testing.T) {
	for seed := int64(1); seed <= 200; seed++ {
		r := rand.New(rand.NewSource(seed))
		e := New()
		e.Apply([]byte("S,ACME,1,1"), nil)
		lg := ledger{given: map[int64]int64{}, accounted: map[int64]int64{}, reordered: map[int64]bool{}}
		nextID := int64(1)
		for range 500 {
			cmd := randomCommand(r, nextID)
			events := e.Apply([]byte(cmd), nil)
			if strings.HasPrefix(string(events), "ACCEPT") {
				nextID++
			}
			lg.observe(t, cmd, events)
			checkBook(t, e, cmd)
		}
		for id, given := range lg.given {
			var open int64
			if o, ok := e.live[id]; ok {
				open = o.open
			}
			if lg.accounted[id]+open != given {
				t.Fatalf("seed %d: order %d was given %d, but %d traded or left and %d is open", seed, id, given, lg.accounted[id], open)
			}
		}
	}
}
