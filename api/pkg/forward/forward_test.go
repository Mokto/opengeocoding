package forward

import (
	"encoding/json"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/proto"
	"testing"

	_ "github.com/go-sql-driver/mysql"
	"github.com/sebdah/goldie/v2"
)

func TestForward(t *testing.T) {
	database := manticoresearch.InitDatabase()

	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 th 2300 Copenhagen S",
		"Geislersgade 14, 3 th 2300 Copenhagen",
		"Geislersgade 14, 3th 2300 Copenhagen",
		"461 W Main St, Cheshire, 06410",
		"The Book Club 100-106 Leonard St Shoreditch London EC2A 4RH, United Kingdom",
		"781 Franklin Ave Crown Heights Brooklyn NYC NY 11216 USA",
		"7926 Old Seward Hwy; Suite A6; Anchorage, Alaska 99518, US",
	}

	for _, query := range queries {
		location, err := Forward(database, query)
		if err != nil {
			panic(err)
		}
		goldenFile(t, query, location)
	}

}

func goldenFile(t *testing.T, name string, location *proto.Location) {
	locationStr, err := json.MarshalIndent(location, "", "  ")
	if err != nil {
		panic(err)
	}

	g := goldie.New(t)
	g.Assert(t, name, locationStr)
}
