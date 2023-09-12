package manticoresearch

import (
	"database/sql"
	"fmt"
	"os"
	"time"
)

type ManticoreSearch struct {
	Balancer *sql.DB
	Worker   *sql.DB
}

func InitDatabase(shouldPing ...bool) *ManticoreSearch {
	shouldPingValue := false
	if shouldPing != nil {
		shouldPingValue = shouldPing[0]
	}

	manticoresearch_balancer_endpoint := os.Getenv("MANTICORESEARCH_BALANCER_ENDPOINT")
	manticoresearch_worker_endpoint := os.Getenv("MANTICORESEARCH_WORKER_ENDPOINT")
	if manticoresearch_balancer_endpoint == "" {
		manticoresearch_balancer_endpoint = "localhost"
	}
	if manticoresearch_worker_endpoint == "" {
		manticoresearch_worker_endpoint = "localhost"
	}

	fmt.Println("Connecting to ManticoreSearch on", manticoresearch_balancer_endpoint)
	database := createDb(manticoresearch_balancer_endpoint, shouldPingValue)
	if manticoresearch_balancer_endpoint == manticoresearch_worker_endpoint {
		return &ManticoreSearch{
			Balancer: database,
			Worker:   database,
		}
	}

	fmt.Println("Connecting to ManticoreSearch on", manticoresearch_worker_endpoint)
	databaseWorker := createDb(manticoresearch_worker_endpoint, shouldPingValue)

	return &ManticoreSearch{
		Balancer: database,
		Worker:   databaseWorker,
	}
}

func createDb(endpoint string, shouldPing bool) *sql.DB {
	database, err := sql.Open("mysql", "tcp("+endpoint+":9306)/")
	if err != nil {
		panic(err)
	}
	database.SetConnMaxLifetime(time.Minute * 3)
	database.SetMaxOpenConns(10)
	database.SetMaxIdleConns(10)

	err = database.Ping()
	if err != nil {
		panic(err)
	}

	return database
}
