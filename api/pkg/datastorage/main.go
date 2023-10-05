package datastorage

import (
	"geocoding/pkg/elasticsearch"
)

type Datastorage struct {
	elasticsearch *elasticsearch.Elasticsearch
}

func InitDatastorage(elasticsearch *elasticsearch.Elasticsearch) *Datastorage {

	datastorage := &Datastorage{
		elasticsearch: elasticsearch,
	}

	datastorage.initCities()
	datastorage.initOpenAddresses()
	datastorage.initOpenstreetdataAddresses()

	return datastorage
}
