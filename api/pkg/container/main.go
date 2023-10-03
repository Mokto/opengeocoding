package container

import (
	"geocoding/pkg/datastorage"
	"geocoding/pkg/elasticsearch"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/messaging"
)

type Container struct {
	GracefulManager *graceful.Manager
	Database        *manticoresearch.ManticoreSearch
	Messaging       *messaging.Messaging
	Datastorage     *datastorage.Datastorage
	Elasticsearch   *elasticsearch.Elasticsearch
}

func Init() *Container {
	geolabels.Load()
	gracefulManager := graceful.Start()

	elasticsearch, err := elasticsearch.New(elasticsearch.Config{})
	if err != nil {
		panic(err)
	}

	database := manticoresearch.InitDatabase(true)

	messaging := messaging.New(gracefulManager)

	datastorage := datastorage.InitDatastorage(database, elasticsearch)

	return &Container{
		GracefulManager: gracefulManager,
		Database:        database,
		Messaging:       messaging,
		Datastorage:     datastorage,
		Elasticsearch:   elasticsearch,
	}
}
