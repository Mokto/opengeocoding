package messaging

func (messaging *Messaging) Publish(queueName string, message string) error {
	err := messaging.publisher.Publish([]byte(message), []string{queueName})

	if err != nil {
		return err
	}
	return nil
}
