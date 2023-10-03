package main

import (
	"fmt"
	"geocoding/pkg/apis"
	"geocoding/pkg/container"
	"geocoding/pkg/forward"
	"time"

	_ "github.com/go-sql-driver/mysql"
	"golang.org/x/sync/errgroup"
)

func main() {

	container := container.Init()
	start := time.Now()

	for j := 1; j < 100; j++ {
		fmt.Println(j, time.Since(start))
		group := errgroup.Group{}

		for i := 1; i < 100; i++ {
			group.Go(func() error {
				_, err := forward.Forward(container, "Copenhagen, Denmark")
				return err
			})
		}
		err := group.Wait()
		if err != nil {
			panic(err)
		}
	}
	fmt.Println("Calls done")
	return

	apis.StartRmqConsumer(container)
	apis.StartGrpc(container)
	apis.StartHttp(container)

	container.GracefulManager.Wait()
}
