// End-to-end tests of the program on the public example sessions and on the line framing rules.

package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func runString(t *testing.T, input string) string {
	t.Helper()
	var out bytes.Buffer
	if err := run(strings.NewReader(input), &out); err != nil {
		t.Fatal(err)
	}
	return out.String()
}

func TestExamples(t *testing.T) {
	inputs, err := filepath.Glob("testdata/*.in")
	if err != nil || len(inputs) == 0 {
		t.Fatalf("no examples found: %v", err)
	}
	for _, in := range inputs {
		t.Run(filepath.Base(in), func(t *testing.T) {
			input, err := os.ReadFile(in)
			if err != nil {
				t.Fatal(err)
			}
			want, err := os.ReadFile(strings.TrimSuffix(in, ".in") + ".out")
			if err != nil {
				t.Fatal(err)
			}
			got := runString(t, string(input))
			if got != string(want) {
				t.Fatalf("output differs\ngot:\n%s\nwant:\n%s", got, want)
			}
			if again := runString(t, string(input)); again != got {
				t.Fatal("the same input gave different output")
			}
		})
	}
}

func TestLineFraming(t *testing.T) {
	cases := []struct{ name, input, want string }{
		{"final line without LF", "S,ACME,1,1\nQ,ACME", "SYMBOL,ACME\nTOP,ACME,0,0,0,0\n"},
		{"CR LF and bare final CR", "S,ACME,1,1\r\nQ,ACME\r", "SYMBOL,ACME\nTOP,ACME,0,0,0,0\n"},
		{"only one CR is removed", "S,ACME,1,1\r\r\n", "REJECT,1,MALFORMED\n"},
		{"empty lines count", "\n\nQ,ACME\n", "REJECT,1,UNKNOWN_COMMAND\nREJECT,2,UNKNOWN_COMMAND\nREJECT,3,UNKNOWN_SYMBOL\n"},
		{"line longer than the read buffer", "Q," + strings.Repeat("A", 200_000) + "\nX\n", "REJECT,1,MALFORMED\nREJECT,2,UNKNOWN_COMMAND\n"},
		{"256 bytes is not too long", "X" + strings.Repeat(",", 255), "REJECT,1,UNKNOWN_COMMAND\n"},
		{"257 bytes is too long", "X" + strings.Repeat(",", 256), "REJECT,1,MALFORMED\n"},
		{"non-ASCII byte", "Q,\xc3\x89\n", "REJECT,1,MALFORMED\n"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := runString(t, c.input); got != c.want {
				t.Fatalf("got %q, want %q", got, c.want)
			}
		})
	}
}
