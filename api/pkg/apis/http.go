package apis

import (
	"context"
	"fmt"
	"geocoding/pkg/container"
	"geocoding/pkg/forward"
	"log"
	"net/http"
	"time"

	"github.com/labstack/echo/v4"
)

func StartHttp(container *container.Container) {

	port := 8090
	server := echo.New()

	server.GET("/healthcheck", func(c echo.Context) error {
		return c.NoContent(http.StatusOK)
	})

	server.GET("/v1/forward", func(c echo.Context) error {
		address := c.QueryParam("address")
		if address == "" {
			return c.NoContent(http.StatusBadRequest)
		}
		res, err := forward.Forward(container, c.QueryParam("address"))
		if err != nil {
			return c.String(http.StatusInternalServerError, err.Error())
		}
		return c.JSON(http.StatusOK, res)
	})

	container.GracefulManager.OnShutdown(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
		defer cancel()
		if err := server.Shutdown(ctx); err != nil {
			server.Logger.Fatal(err)
		}
	})

	// start gRPC server
	go func() {
		err := server.Start(fmt.Sprintf(":%d", port))
		if err != nil && err.Error() != "http: Server closed" {
			log.Fatal(err)
		}
	}()
}
