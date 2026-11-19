// Command engine reads order commands from standard input and writes matching events to standard output.
package main

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"

	"benchy/engine/matching"
)

const usage = `usage: engine < commands > events

Reads one command per line from standard input until end of input,
and writes the events of each command to standard output.
`

func main() {
	switch {
	case len(os.Args) == 2 && os.Args[1] == "--help":
		fmt.Print(usage)
		return
	case len(os.Args) > 1:
		fmt.Fprint(os.Stderr, usage)
		os.Exit(2)
	}
	if err := run(os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "engine:", err)
		os.Exit(1)
	}
}

// run feeds every line of r to a new engine and writes its events to w.
func run(r io.Reader, w io.Writer) error {
	in := bufio.NewReaderSize(r, 64*1024)
	out := bufio.NewWriterSize(w, 64*1024)
	engine := matching.New()
	var events []byte
	for {
		line, err := readLine(in)
		if err == io.EOF {
			break
		}
		if err != nil {
			return fmt.Errorf("read input: %w", err)
		}
		events = engine.Apply(line, events[:0])
		if _, err := out.Write(events); err != nil {
			return fmt.Errorf("write output: %w", err)
		}
	}
	if err := out.Flush(); err != nil {
		return fmt.Errorf("flush output: %w", err)
	}
	return nil
}

// readLine gives the next line without its LF and one trailing CR, or io.EOF
// when no bytes are left. A line longer than the read buffer is cut to the
// buffer size, which is still far over the protocol limit, so it is rejected
// the same way.
func readLine(in *bufio.Reader) ([]byte, error) {
	line, err := in.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) {
		long := bytes.Clone(line)
		for errors.Is(err, bufio.ErrBufferFull) {
			_, err = in.ReadSlice('\n')
		}
		if err != nil && err != io.EOF {
			return nil, err
		}
		return long, nil
	}
	switch {
	case err == io.EOF && len(line) > 0:
	case err != nil:
		return nil, err
	default:
		line = line[:len(line)-1]
	}
	return bytes.TrimSuffix(line, []byte{'\r'}), nil
}
