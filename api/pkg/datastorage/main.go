package datastorage

import (
	"geocoding/pkg/manticoresearch"
)

type Datastorage struct {
	database *manticoresearch.ManticoreSearch
}

func InitDatastorage(database *manticoresearch.ManticoreSearch) *Datastorage {

	datastorage := &Datastorage{
		database: database,
	}

	datastorage.initCities()
	datastorage.initOpenAddresses()
	datastorage.initOpenstreetdataAddresses()

	return datastorage
}
