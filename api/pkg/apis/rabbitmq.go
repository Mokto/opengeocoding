package apis

import (
	"fmt"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"
	"log"

	"github.com/wagslane/go-rabbitmq"
)

func StartRmqConsumer(gracefulManager *graceful.Manager, database *manticoresearch.ManticoreSearch, rmqConnection *rabbitmq.Conn) {

	consumer, err := rabbitmq.NewConsumer(
		rmqConnection,
		func(d rabbitmq.Delivery) rabbitmq.Action {
			_, err := database.Worker.Exec(string(d.Body))
			if err != nil {
				fmt.Println(err)
				if !d.Redelivered {
					return rabbitmq.NackRequeue
				} else {
					return rabbitmq.NackDiscard
				}
			}
			return rabbitmq.Ack
		},
		"main:::backgroundSave",
		rabbitmq.WithConsumerOptionsConcurrency(1),
		rabbitmq.WithConsumerOptionsQueueQuorum,
		rabbitmq.WithConsumerOptionsQueueDurable,
	)
	if err != nil {
		log.Fatal(err)
	}

	gracefulManager.OnShutdown(func() {
		consumer.Close()
	})
}

type RmqPublisher struct {
	publisher *rabbitmq.Publisher
}

func (publisher *RmqPublisher) Publish(queueName string, message string) error {
	err := publisher.publisher.Publish([]byte(message), []string{queueName}) // rabbitmq.WithPublishOptionsExchange("events"),

	if err != nil {
		return err
	}
	return nil
}

func StartRmqPublisher(gracefulManager *graceful.Manager, rmqConnection *rabbitmq.Conn) *RmqPublisher {

	publisher, err := rabbitmq.NewPublisher(
		rmqConnection,
	)
	if err != nil {
		log.Fatal(err)
	}

	gracefulManager.OnShutdown(func() {
		defer publisher.Close()
	})

	return &RmqPublisher{
		publisher: publisher,
	}
}
