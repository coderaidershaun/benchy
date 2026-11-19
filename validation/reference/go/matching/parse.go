// Field rules of the protocol, which decide MALFORMED from the line alone.

package matching

const (
	maxLineLen = 256
	maxFields  = 8
	maxID      = 999_999_999_999_999_999
	maxValue   = 1_000_000_000
)

// fieldCounts gives the number of fields each command code takes.
var fieldCounts = map[string]int{
	"S": 4, "N": 8, "M": 6, "I": 8, "C": 2, "A": 4, "Q": 2, "D": 3,
}

// fields is a line split at its commas. n counts every field, even past maxFields.
type fields struct {
	f [maxFields][]byte
	n int
}

func splitFields(line []byte) fields {
	var fs fields
	start := 0
	for i := 0; i <= len(line); i++ {
		if i < len(line) && line[i] != ',' {
			continue
		}
		if fs.n < maxFields {
			fs.f[fs.n] = line[start:i]
		}
		fs.n++
		start = i + 1
	}
	return fs
}

// parseInt reads 1 to 18 ASCII digits with a value from 1 to limit.
func parseInt(b []byte, limit int64) (int64, bool) {
	if len(b) == 0 || len(b) > 18 {
		return 0, false
	}
	var v int64
	for _, c := range b {
		if c < '0' || c > '9' {
			return 0, false
		}
		v = v*10 + int64(c-'0')
	}
	return v, v >= 1 && v <= limit
}

func parseName(b []byte) (string, bool) {
	if len(b) == 0 || len(b) > 8 {
		return "", false
	}
	for _, c := range b {
		if (c < 'A' || c > 'Z') && (c < '0' || c > '9') {
			return "", false
		}
	}
	return string(b), true
}

func parseSide(b []byte) (Side, bool) {
	if len(b) == 1 && (b[0] == byte(Buy) || b[0] == byte(Sell)) {
		return Side(b[0]), true
	}
	return 0, false
}

// TimeInForce says what a limit order does with quantity that does not trade on entry.
type TimeInForce int

const (
	GTC TimeInForce = iota + 1
	IOC
	FOK
	Post
)

func parseTIF(b []byte) (TimeInForce, bool) {
	switch string(b) {
	case "GTC":
		return GTC, true
	case "IOC":
		return IOC, true
	case "FOK":
		return FOK, true
	case "POST":
		return Post, true
	}
	return 0, false
}

// fieldParser reads typed fields in order and remembers whether any failed.
type fieldParser struct {
	fs fields
	i  int
	ok bool
}

func (p *fieldParser) next() []byte {
	p.i++
	return p.fs.f[p.i]
}

func (p *fieldParser) int(limit int64) int64 {
	v, ok := parseInt(p.next(), limit)
	p.ok = p.ok && ok
	return v
}

func (p *fieldParser) name() string {
	v, ok := parseName(p.next())
	p.ok = p.ok && ok
	return v
}

func (p *fieldParser) side() Side {
	v, ok := parseSide(p.next())
	p.ok = p.ok && ok
	return v
}

func (p *fieldParser) tif() TimeInForce {
	v, ok := parseTIF(p.next())
	p.ok = p.ok && ok
	return v
}
