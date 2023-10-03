package datastorage

import (
	"geocoding/pkg/elasticsearch"
	"geocoding/pkg/manticoresearch"
)

type Datastorage struct {
	database      *manticoresearch.ManticoreSearch
	elasticsearch *elasticsearch.Elasticsearch
}

func InitDatastorage(database *manticoresearch.ManticoreSearch, elasticsearch *elasticsearch.Elasticsearch) *Datastorage {

	datastorage := &Datastorage{
		database:      database,
		elasticsearch: elasticsearch,
	}

	datastorage.initCities()
	datastorage.initOpenAddresses()
	datastorage.initOpenstreetdataAddresses()

	return datastorage
}
