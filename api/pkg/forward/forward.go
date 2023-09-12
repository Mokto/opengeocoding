package forward

import (
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"strings"
)

// Make the string SQL safe
func escape_sql(s string) string {
	return strings.ReplaceAll(s, "'", "\\'")
}

func Forward(database *manticoresearch.ManticoreSearch, address string) (*proto.ForwardResult, error) {
	parsed := parser.ParseAddress(address)

	if parsed.Road == nil && parsed.House == "" {
		if parsed.City == nil && parsed.Country == "" {
			return &proto.ForwardResult{}, nil
		}
		return forwardCity(database, parsed)
	}

	return forwardFull(database, parsed)
}
