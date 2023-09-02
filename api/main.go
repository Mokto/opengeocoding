package main

import (
	context "context"
	"database/sql"
	"fmt"
	"log"
	"net"
	"strings"
	"time"

	"libpostal/proto"

	_ "github.com/go-sql-driver/mysql"

	expand "github.com/openvenues/gopostal/expand"

	grpc "google.golang.org/grpc"
	"google.golang.org/grpc/reflection"
)

type opengeocodingServer struct {
	database *sql.DB
	proto.UnimplementedOpenGeocodingServer
}

func (s *opengeocodingServer) Forward(ctx context.Context, request *proto.ForwardRequest) (*proto.ForwardResponse, error) {
	options := expand.GetDefaultExpansionOptions()
	options.Languages = []string{"en"}
	allAddresses := expand.ExpandAddressOptions(request.Address, options)

	matches := []string{}
	for _, address := range allAddresses {
		matches = append(matches, fmt.Sprintf(`"%s"/0.6`, address))
	}

	rows, err := s.database.Query(`SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM openaddresses WHERE MATCH('` + strings.Join(matches, "|") + `') limit 1`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	for rows.Next() {
		var (
			street       string
			number       string
			unit         string
			city         string
			district     string
			region       string
			postcode     string
			lat          float32
			long         float32
			country_code string
		)
		if err := rows.Scan(&street, &number, &unit, &city, &district, &region, &postcode, &lat, &long, &country_code); err != nil {
			log.Fatal(err)
		}
		return &proto.ForwardResponse{
			Location: &proto.Location{
				Street:      &street,
				Number:      &number,
				Unit:        &unit,
				City:        &city,
				District:    &district,
				Region:      &region,
				Postcode:    &postcode,
				Lat:         &lat,
				Long:        &long,
				CountryCode: &country_code,
			},
		}, nil
		// log.Printf("id %d street is %s\n", id, street)
	}

	return &proto.ForwardResponse{}, nil
}

func main() {

	fmt.Println("Connecting to database")
	database, err := sql.Open("mysql", "tcp(localhost:9306)/")
	if err != nil {
		panic(err)
	}
	database.SetConnMaxLifetime(time.Minute * 3)
	database.SetMaxOpenConns(10)
	database.SetMaxIdleConns(10)

	fmt.Println("Ping")
	err = database.Ping()
	if err != nil {
		panic(err)
	}
	fmt.Println("Ok")

	port := 8091
	lis, err := net.Listen("tcp", fmt.Sprintf("localhost:%d", port))
	if err != nil {
		log.Fatalf("failed to listen: %v", err)
	}
	grpcServer := grpc.NewServer()
	proto.RegisterOpenGeocodingServer(grpcServer, &opengeocodingServer{
		database: database,
	})
	reflection.Register(grpcServer)
	fmt.Println("Serving GRPC server on port", port)
	grpcServer.Serve(lis)
}
