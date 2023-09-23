package forward

import (
	"geocoding/pkg/container"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"strings"
)

// Make the string SQL safe
func escape_sql(s string) string {
	characters := []string{"\\", "'", "/", "!", `"`, "$", "(", ")", "-", "<", "@", "^", "|", "~"}
	for _, character := range characters {
		s = strings.ReplaceAll(s, character, "\\"+character)
	}
	return s
}

func Forward(container *container.Container, address string) (*proto.ForwardResult, error) {
	parsed := parser.ParseAddress(address)

	if parsed.Road == nil && parsed.House == "" {
		if parsed.City == nil && parsed.Country == "" {
			return &proto.ForwardResult{}, nil
		}
		return forwardCity(container, parsed)
	}

	return forwardFull(container, parsed)
}
