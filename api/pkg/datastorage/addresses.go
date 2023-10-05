package datastorage

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"geocoding/pkg/errors"
	"geocoding/pkg/proto"
)

var openaddressesTableName = "openaddresses"
var openstreetdataAddressesTableName = "openstreetdata_addresses"

func (datastorage *Datastorage) initOpenAddresses() {
	datastorage.initAddressesTable(openaddressesTableName)
}

func (datastorage *Datastorage) initOpenstreetdataAddresses() {
	datastorage.initAddressesTable(openstreetdataAddressesTableName)
}

func (datastorage *Datastorage) initAddressesTable(tableName string) {

	err := datastorage.elasticsearch.CreateIndexIfNotExists(tableName, `{
		"settings": {
		  "number_of_shards": 2,
		  "number_of_replicas": 2,
		  "index.refresh_interval": "30s",
		  "analysis": {
			"analyzer": {
			  "standard_asciifolding": {
				"tokenizer": "standard",
				"filter": [ "asciifolding", "lowercase" ]
			  }
			}
		  }
		},
		"mappings": {
		  "dynamic": "strict",
		  "properties": {
			"street": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"city": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"region": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"number": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"unit": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"district": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"postcode": {
			  "type": "text",
			  "analyzer": "standard_asciifolding"
			},
			"location": {
			  "type": "geo_point"
			},
			"country_code":{
			  "type": "keyword"
			}
		  }
		}
	  }
	  `)
	if err != nil {
		panic(err)
	}
}

type Address struct {
	City        string    `json:"city,omitempty"`
	Region      string    `json:"region,omitempty"`
	Location    *Location `json:"location,omitempty"`
	CountryCode string    `json:"country_code,omitempty"`
	Street      string    `json:"street,omitempty"`
	Number      string    `json:"number,omitempty"`
	Unit        string    `json:"unit,omitempty"`
	District    string    `json:"district,omitempty"`
	Postcode    string    `json:"postcode,omitempty"`
}

func (datastorage *Datastorage) InsertAddresses(locations []*proto.Location, source proto.Source) error {
	tableName := ""
	if source == proto.Source_OpenStreetDataAddress {
		tableName = openstreetdataAddressesTableName
	} else if source == proto.Source_OpenAddresses {
		tableName = openaddressesTableName
	} else {
		return fmt.Errorf("source not supported for addresses %s", source)
	}

	values := map[string]string{}

	for _, location := range locations {
		if location.Lat > 90 || location.Lat < -90 || location.Long > 180 || location.Long < -180 {
			// fmt.Println("not valid", location)
			continue
		}
		address := &Address{
			City:        *location.City,
			Region:      *location.Region,
			Location:    &Location{Latitude: location.Lat, Longitude: location.Long},
			CountryCode: *location.CountryCode,
			Street:      *location.Street,
			Number:      *location.Number,
			Unit:        *location.Unit,
			District:    *location.District,
			Postcode:    *location.Postcode,
		}
		element, err := json.Marshal(address)
		if err != nil {
			return errors.Wrap(err)
		}
		h := sha256.New()
		h.Write([]byte(*location.Id))
		values[fmt.Sprintf("%x", h.Sum(nil))] = string(element)
	}

	err := datastorage.elasticsearch.BulkInsertDocuments(context.Background(), tableName, values)

	if err != nil {
		return errors.Wrap(err)
	}

	return nil

}
