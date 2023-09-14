package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/messaging"

	_ "github.com/go-sql-driver/mysql"
)

func main() {

	gracefulManager := graceful.Start()

	database := manticoresearch.InitDatabase(true)

	messaging := messaging.New(gracefulManager)

	apis.StartRmqConsumer(gracefulManager, database, messaging)
	apis.StartGrpc(gracefulManager, database, messaging)
	apis.StartHttp(gracefulManager, database)

	gracefulManager.Wait()
}
