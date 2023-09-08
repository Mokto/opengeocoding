package forward

import (
	"database/sql"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"strings"
)

// Make the string SQL safe
func escape_sql(s string) string {
	return strings.ReplaceAll(s, "'", "\\'")
}

func Forward(database *sql.DB, address string) (*proto.ForwardResult, error) {
	parsed := parser.ParseAddress(address)

	if parsed.Road == "" && parsed.House == "" {
		if parsed.City == "" && parsed.Country == "" {
			return &proto.ForwardResult{}, nil
		}
		return forwardCity(database, parsed)
	}

	return forwardFull(database, parsed)
}
