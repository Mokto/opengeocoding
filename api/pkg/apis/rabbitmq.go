package apis

import (
	"fmt"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/messaging"
	"log"

	"github.com/wagslane/go-rabbitmq"
)

func StartRmqConsumer(gracefulManager *graceful.Manager, database *manticoresearch.ManticoreSearch, messaging *messaging.Messaging) {

	consumer, err := rabbitmq.NewConsumer(
		messaging.Connection,
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
		"main:::opengeocoding:backgroundSave",
		rabbitmq.WithConsumerOptionsConcurrency(1),
		rabbitmq.WithConsumerOptionsQueueQuorum,
		rabbitmq.WithConsumerOptionsQueueDurable,
		rabbitmq.WithConsumerOptionsQueueArgs(rabbitmq.Table{
			"x-dead-letter-exchange": "dlx:::opengeocoding:backgroundSave",
		}),
	)
	if err != nil {
		log.Fatal(err)
	}

	gracefulManager.OnShutdown(func() {
		consumer.Close()
	})
}
