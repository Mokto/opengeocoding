package errors

import (
	"strings"
)

// GetStack return each stack line separately
func GetStack(err error) []string {
	return strings.Split(SprintSource(err), "\n\n")
}
