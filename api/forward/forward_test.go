package forward

import (
	"encoding/json"
	"geocoding/manticoresearch"
	"geocoding/proto"
	"testing"

	_ "github.com/go-sql-driver/mysql"
	"github.com/sebdah/goldie/v2"
)

func TestForward(t *testing.T) {
	database := manticoresearch.InitDatabase()

	queries := []string{
		"Geislersgade 14, 4 2300 Copenhagen",
		"Geislersgade 14, 3 mf 2300 Copenhagen S",
		"461 W Main St, Cheshire, 06410",
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
