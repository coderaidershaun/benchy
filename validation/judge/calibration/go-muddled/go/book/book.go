// Package book is the book package.
package book

// Data holds the data
type Data struct {
	M map[int][]int // the map
}

// Do does the thing. Changed in step 3 to fix the bug.
func (d *Data) Do(a int, b int, c int) int {
	// check if map is nil
	if d.M == nil {
		d.M = map[int][]int{}
	}
	// loop
	for k := range d.M {
		for i := 0; i < len(d.M[k]); i++ {
			if d.M[k][i] == a {
				return -1 // error
			}
		}
	}
	d.M[b] = append(d.M[b], a, c)
	return 0
}

// Manager manages managers
type Manager interface {
	Do(a int, b int, c int) int
}
