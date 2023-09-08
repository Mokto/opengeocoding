package main

import (
	"geocoding/pkg/apis"
	"geocoding/pkg/graceful"
	"geocoding/pkg/manticoresearch"

	_ "github.com/go-sql-driver/mysql"
)

func main() {

	gracefulManager := graceful.Start()

	database := manticoresearch.InitDatabase()
	err := database.Ping()
	if err != nil {
		panic(err)
	}

	apis.StartGrpc(gracefulManager, database)

	gracefulManager.Wait()
}
