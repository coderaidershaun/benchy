// Book logic tests: worked sessions where a command and the events it writes sit side by side.

package matching

import (
	"strings"
	"testing"
)

// checkSession runs a session written as "> command" lines, each followed by
// the events that command must write.
func checkSession(t *testing.T, session string) {
	t.Helper()
	e := New()
	var got []byte
	var want strings.Builder
	for _, line := range strings.Split(strings.TrimSpace(session), "\n") {
		line = strings.TrimSpace(line)
		if cmd, ok := strings.CutPrefix(line, "> "); ok {
			got = e.Apply([]byte(cmd), got)
			continue
		}
		want.WriteString(line + "\n")
	}
	if string(got) != want.String() {
		t.Fatalf("events differ\ngot:\n%s\nwant:\n%s", got, want.String())
	}
}

func TestPriceTimePriority(t *testing.T) {
	checkSession(t, `
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
		> Q,ACME
		TOP,ACME,0,0,101,5`)
}

func TestSelfTradeCancelsRestingAndKeepsMatching(t *testing.T) {
	checkSession(t, `
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
		TOP,ACME,101,5,0,0`)
}

func TestFillOrKillIgnoresOwnOrdersAndRemovesOnlyThoseItMeets(t *testing.T) {
	checkSession(t, `
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
		TOP,ACME,0,0,100,10`)
}

func TestFillOrKillCountsHiddenQuantity(t *testing.T) {
	checkSession(t, `
		> S,ACME,1,1
		SYMBOL,ACME
		> I,1,ACME,S,100,30,10,AL
		ACCEPT,1
		> N,2,ACME,B,100,25,FOK,BO
		ACCEPT,2
		TRADE,ACME,100,10,2,1
		TRADE,ACME,100,10,2,1
		TRADE,ACME,100,5,2,1
		> D,ACME,5
		DEPTH,ACME,0,1
		LEVEL,S,100,5,1`)
}

func TestIcebergRefillMovesToBackEvenWhenIncomingIsFilled(t *testing.T) {
	checkSession(t, `
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
		TOP,ACME,0,0,100,10`)
}

func TestIcebergKeepsPeakThroughAmends(t *testing.T) {
	checkSession(t, `
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
		TOP,ACME,0,0,100,8`)
}

func TestAmendToNewPriceMovesLevelAndRejectsCrossing(t *testing.T) {
	checkSession(t, `
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
		> D,ACME,5
		DEPTH,ACME,1,1
		LEVEL,B,98,10,1
		LEVEL,S,100,10,1`)
}

func TestCheckOrderDecidesTheCode(t *testing.T) {
	checkSession(t, `
		> S,ACME,5,10
		SYMBOL,ACME
		> N,1,ACME,B,100,10,GTC,AL
		ACCEPT,1
		> N,1,NOPE,B,3,15,GTC,AL
		REJECT,3,DUPLICATE_ORDER_ID
		> N,2,ACME,B,3,15,GTC,AL
		REJECT,4,INVALID_PRICE
		> I,2,ACME,B,5,20,15,AL
		REJECT,5,INVALID_QTY
		> I,2,ACME,B,5,10,20,AL
		REJECT,6,MALFORMED
		> A,1,100,0
		REJECT,7,MALFORMED
		> C,2
		REJECT,8,UNKNOWN_ORDER
		> Q,ACME,
		REJECT,9,MALFORMED
		> N,3,ACME,B,005,010,GTC,AL
		ACCEPT,3`)
}
