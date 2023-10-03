package datastorage

import (
	"context"
	"encoding/json"
	"geocoding/pkg/errors"
	"geocoding/pkg/proto"
)

var citiesTableName = "geonames_cities"

type Location struct {
	Latitude  float32 `json:"lat"`
	Longitude float32 `json:"lon"`
}

type City struct {
	City        string    `json:"city"`
	Region      string    `json:"region"`
	Location    *Location `json:"location"`
	CountryCode string    `json:"country_code"`
	Population  int       `json:"population"`
}

func (datastorage *Datastorage) initCities() {
	err := datastorage.elasticsearch.CreateIndexIfNotExists(citiesTableName, `{
		"settings": {
		  "number_of_shards": 2,
		  "number_of_replicas": 0
		},
		"mappings": {
		  "dynamic": "strict",
		  "properties": {
			"city": {
			  "type": "text"
			},
			"region": {
			  "type": "text"
			},
			"location": {
			  "type": "geo_point"
			},
			"country_code":{
			  "type": "keyword"
			},
			"population": {
			  "type": "integer"
			}
		  }
		}
	  }
	  `)
	if err != nil {
		panic(err)
	}
}

func (datastorage *Datastorage) InsertCities(locations []*proto.Location) error {
	values := map[string]string{}

	for _, location := range locations {
		city := &City{
			City:        *location.City,
			Region:      *location.Region,
			Location:    &Location{Latitude: location.Lat, Longitude: location.Long},
			CountryCode: *location.CountryCode,
			Population:  int(*location.Population),
		}
		element, err := json.Marshal(city)
		if err != nil {
			return errors.Wrap(err)
		}
		values[*location.Id] = string(element)
	}

	err := datastorage.elasticsearch.BulkInsertDocuments(context.Background(), citiesTableName, values)

	if err != nil {
		return errors.Wrap(err)
	}

	return nil

}
