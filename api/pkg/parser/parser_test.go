package parser

import (
	"encoding/json"
	"testing"

	"github.com/bradleyjkemp/cupaloy"
	_ "github.com/go-sql-driver/mysql"
)

func TestParser(t *testing.T) {
	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 th 2300 Copenhagen S",
		"Geislersgade 14, 3 th 2300 Copenhagen",
		"Geislersgade 14, 3th 2300 Copenhagen",
		"461 W Main St, Cheshire, 06410",
		"The Book Club 100-106 Leonard St Shoreditch London EC2A 4RH, United Kingdom",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
		"Lawrence, Kansas, United States",
		"Metz, Lorraine, France",
		"178 Columbus Avenue; #231573; New York, NY 10023, US, United States",
		"178 Columbus Avenue; #231573; New York, NY 10023, United States",
		"Shangjiangcheng Industrial Zone; Dongguan 523000, China",
		"CDMX, CDMX, MX, Mexico",
		"Wellington, Florida 33614, US, United States",
		"Arnhem, NL, Netherlands",
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

	t.Run(name, func(t *testing.T) {
		cupaloy.SnapshotT(t, addressStr)
	})
}
