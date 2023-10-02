package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/container"

	_ "github.com/go-sql-driver/mysql"
)

func main() {

	container := container.Init()

	apis.StartRmqConsumer(container)
	apis.StartGrpc(container)
	apis.StartHttp(container)

	container.GracefulManager.Wait()
}
