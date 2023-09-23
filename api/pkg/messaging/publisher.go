package messaging

import (
	"time"

	"github.com/avast/retry-go"
)

func (messaging *Messaging) Publish(queueName string, message string) error {
	return messaging.PublishBytes(queueName, []byte(message))
}

func (messaging *Messaging) PublishBytes(queueName string, message []byte) error {
	err := retry.Do(
		func() error {
			err := messaging.publisher.Publish(message, []string{queueName})

			if err != nil {
				return err
			}

			return nil
		},
		retry.Delay(time.Second*5),
	)
	if err != nil {
		return err
	}

	return nil
}
