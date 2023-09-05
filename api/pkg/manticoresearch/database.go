package manticoresearch

import (
	"database/sql"
	"fmt"
	"os"
	"time"
)

func InitDatabase() *sql.DB {
	manticoresearch_endpoint := os.Getenv("MANTICORESEARCH_ENDPOINT")
	if manticoresearch_endpoint == "" {
		manticoresearch_endpoint = "localhost"
	}
	fmt.Println("Connecting to ManticoreSearch on", manticoresearch_endpoint)

	database, err := sql.Open("mysql", "tcp("+manticoresearch_endpoint+":9306)/")
	if err != nil {
		panic(err)
	}
	database.SetConnMaxLifetime(time.Minute * 3)
	database.SetMaxOpenConns(10)
	database.SetMaxIdleConns(10)

	return database
}
