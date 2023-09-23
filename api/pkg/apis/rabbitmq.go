package apis

import (
	"fmt"
	"geocoding/pkg/container"
	"geocoding/pkg/proto"
	"log"

	"github.com/wagslane/go-rabbitmq"
	goproto "google.golang.org/protobuf/proto"
)

func backgroundSave(container *container.Container) *rabbitmq.Consumer {

	consumer, err := rabbitmq.NewConsumer(
		container.Messaging.Connection,
		func(d rabbitmq.Delivery) rabbitmq.Action {
			_, err := container.Database.Worker.Exec(string(d.Body))
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
		rabbitmq.WithConsumerOptionsConcurrency(10),
		rabbitmq.WithConsumerOptionsQOSPrefetch(5),
		rabbitmq.WithConsumerOptionsQueueDurable,
		rabbitmq.WithConsumerOptionsQueueArgs(rabbitmq.Table{
			"x-dead-letter-exchange": "dlx:::opengeocoding:backgroundSave",
		}),
		rabbitmq.WithConsumerOptionsQueueQuorum,
	)
	if err != nil {
		log.Fatal(err)
	}

	return consumer
}

func insertDocuments(container *container.Container) *rabbitmq.Consumer {

	consumer, err := rabbitmq.NewConsumer(
		container.Messaging.Connection,
		func(d rabbitmq.Delivery) rabbitmq.Action {
			var err error
			request := &proto.InsertLocationsRequest{}
			err = goproto.Unmarshal(d.Body, request)
			if err != nil {
				fmt.Println(err)
				return rabbitmq.NackDiscard
			}

			if request.Locations[0].Source == proto.Source_Geonames {
				err = container.Datastorage.InsertCities(request.Locations)
			} else if request.Locations[0].Source == proto.Source_OpenAddresses {
				err = container.Datastorage.InsertAddresses(request.Locations, proto.Source_OpenAddresses)
			} else if request.Locations[0].Source == proto.Source_OpenStreetDataAddress {
				err = container.Datastorage.InsertAddresses(request.Locations, proto.Source_OpenStreetDataAddress)
			} else {
				fmt.Println("Unknown source")
				return rabbitmq.NackDiscard
			}

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
		"main:::opengeocoding:insertDocuments",
		rabbitmq.WithConsumerOptionsConcurrency(10),
		rabbitmq.WithConsumerOptionsQOSPrefetch(5),
		rabbitmq.WithConsumerOptionsQueueDurable,
		rabbitmq.WithConsumerOptionsQueueArgs(rabbitmq.Table{
			"x-dead-letter-exchange": "dlx:::opengeocoding:insertDocuments",
		}),
		rabbitmq.WithConsumerOptionsQueueQuorum,
	)
	if err != nil {
		log.Fatal(err)
	}

	return consumer
}

func StartRmqConsumer(container *container.Container) {

	backgroundSaveConsumer := backgroundSave(container)
	insertDocumentsConsumer := insertDocuments(container)

	container.GracefulManager.OnShutdown(func() {
		backgroundSaveConsumer.Close()
		insertDocumentsConsumer.Close()
	})
}
