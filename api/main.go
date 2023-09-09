package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"
	"log"

	_ "github.com/go-sql-driver/mysql"
	"github.com/wagslane/go-rabbitmq"
)

func main() {

	gracefulManager := graceful.Start()

	database := manticoresearch.InitDatabase()
	err := database.Ping()
	if err != nil {
		panic(err)
	}
	rmqConnection, err := rabbitmq.NewConn(
		"amqp://guest:guest@localhost",
		rabbitmq.WithConnectionOptionsLogging,
	)
	if err != nil {
		log.Fatal(err)
	}
	defer rmqConnection.Close()

	publisher := apis.StartRmqPublisher(gracefulManager, rmqConnection)

	apis.StartRmqConsumer(gracefulManager, database, rmqConnection)
	apis.StartGrpc(gracefulManager, database, publisher)
	apis.StartHttp(gracefulManager, database)

	gracefulManager.Wait()
}
