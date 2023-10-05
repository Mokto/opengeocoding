package container

import (
	"geocoding/pkg/datastorage"
	"geocoding/pkg/elasticsearch"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/graceful"
	"geocoding/pkg/messaging"
)

type Container struct {
	GracefulManager *graceful.Manager
	Messaging       *messaging.Messaging
	Datastorage     *datastorage.Datastorage
	Elasticsearch   *elasticsearch.Elasticsearch
}

func Init() *Container {
	geolabels.Load()
	gracefulManager := graceful.Start()

	elasticsearch := elasticsearch.InitDatabase()

	messaging := messaging.New(gracefulManager)

	datastorage := datastorage.InitDatastorage(elasticsearch)

	return &Container{
		GracefulManager: gracefulManager,
		Messaging:       messaging,
		Datastorage:     datastorage,
		Elasticsearch:   elasticsearch,
	}
}
