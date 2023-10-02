package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/container"
	"geocoding/pkg/geolabels"

	_ "github.com/go-sql-driver/mysql"
)

func main() {

	geolabels.BuildCityLabels()

	container := container.Init()

	apis.StartRmqConsumer(container)
	apis.StartGrpc(container)
	apis.StartHttp(container)

	container.GracefulManager.Wait()
}
