package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/config"
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
	protocol := "amqp"
	if config.GetEnvAsBool("RABBITMQ_SSL", false) {
		protocol = "amqps"
	}
	rmqConnection, err := rabbitmq.NewConn(
		protocol+"://"+config.GetEnv("RABBITMQ_USER", "guest")+":"+config.GetEnv("RABBITMQ_PASSWORD", "guest")+"@"+config.GetEnv("RABBITMQ_HOST", "localhost")+":"+config.GetEnv("RABBITMQ_PORT", "5672"),
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
