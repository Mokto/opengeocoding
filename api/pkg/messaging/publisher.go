package messaging

import (
	"time"

	"github.com/avast/retry-go"
)

func (messaging *Messaging) Publish(queueName string, message string) error {
	err := retry.Do(
		func() error {
			err := messaging.publisher.Publish([]byte(message), []string{queueName})

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
