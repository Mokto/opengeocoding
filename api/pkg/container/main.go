package container

import (
	"geocoding/pkg/datastorage"
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
}

func Init() *Container {
	geolabels.Load()
	gracefulManager := graceful.Start()

	database := manticoresearch.InitDatabase(true)

	messaging := messaging.New(gracefulManager)

	datastorage := datastorage.InitDatastorage(database)

	return &Container{
		GracefulManager: gracefulManager,
		Database:        database,
		Messaging:       messaging,
		Datastorage:     datastorage,
	}
}
