package messaging

import (
	"geocoding/pkg/config"
	"geocoding/pkg/graceful"
	"log"

	amqp "github.com/rabbitmq/amqp091-go"
	"github.com/wagslane/go-rabbitmq"
)

type Messaging struct {
	Connection *rabbitmq.Conn
	publisher  *rabbitmq.Publisher
}

func New(gracefulManager *graceful.Manager) *Messaging {
	protocol := "amqp"
	if config.GetEnvAsBool("RABBITMQ_SSL", false) {
		protocol = "amqps"
	}
	rabbitUrl := protocol + "://" + config.GetEnv("RABBITMQ_USER", "guest") + ":" + config.GetEnv("RABBITMQ_PASSWORD", "guest") + "@" + config.GetEnv("RABBITMQ_HOST", "localhost") + ":" + config.GetEnv("RABBITMQ_PORT", "5672")
	rmqConnection, err := rabbitmq.NewConn(rabbitUrl)
	if err != nil {
		log.Fatal(err)
	}

	declareDlx(rabbitUrl)

	publisher, err := rabbitmq.NewPublisher(
		rmqConnection,
	)
	if err != nil {
		log.Fatal(err)
	}

	gracefulManager.OnShutdown(func() {
		defer publisher.Close()
	})

	return &Messaging{
		Connection: rmqConnection,
		publisher:  publisher,
	}
}

func declareDlx(rabbitmqUrl string) {

	connection, err := amqp.Dial(rabbitmqUrl)
	if err != nil {
		log.Fatal(err)
	}
	channel, err := connection.Channel()
	if err != nil {
		log.Fatal(err)
	}
	err = channel.ExchangeDeclare("dlx:::opengeocoding:backgroundSave", "fanout", true, false, false, false, nil)
	if err != nil {
		log.Fatal(err)
	}
	_, err = channel.QueueDeclare("dlx:::opengeocoding:backgroundSave", true, false, false, false, amqp.Table{"x-queue-type": "quorum"})
	if err != nil {
		log.Fatal(err)
	}
	err = channel.QueueBind("dlx:::opengeocoding:backgroundSave", "dlx:::opengeocoding:backgroundSave", "dlx:::opengeocoding:backgroundSave", false, nil)
	if err != nil {
		log.Fatal(err)
	}
	err = channel.ExchangeDeclare("dlx:::opengeocoding:insertDocuments", "fanout", true, false, false, false, nil)
	if err != nil {
		log.Fatal(err)
	}
	_, err = channel.QueueDeclare("dlx:::opengeocoding:insertDocuments", true, false, false, false, amqp.Table{"x-queue-type": "quorum"})
	if err != nil {
		log.Fatal(err)
	}
	err = channel.QueueBind("dlx:::opengeocoding:insertDocuments", "dlx:::opengeocoding:insertDocuments", "dlx:::opengeocoding:insertDocuments", false, nil)
	if err != nil {
		log.Fatal(err)
	}
	channel.Close()
}
