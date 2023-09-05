package parser

import (
	"encoding/json"
	"testing"

	_ "github.com/go-sql-driver/mysql"
	"github.com/sebdah/goldie/v2"
)

func TestForward(t *testing.T) {
	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 th 2300 Copenhagen S",
		"Geislersgade 14, 3 th 2300 Copenhagen",
		"Geislersgade 14, 3th 2300 Copenhagen",
		"461 W Main St, Cheshire, 06410",
		"The Book Club 100-106 Leonard St Shoreditch London EC2A 4RH, United Kingdom",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
	}

	for _, query := range queries {
		address := ParseAddress(query)
		goldenFile(t, query, address)
	}

}

func goldenFile(t *testing.T, name string, address ParsedAddress) {
	addressStr, err := json.MarshalIndent(address, "", "  ")
	if err != nil {
		panic(err)
	}

	g := goldie.New(t)
	g.Assert(t, name, addressStr)
}
