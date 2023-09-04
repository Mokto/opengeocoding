package main

import (
	context "context"
	"database/sql"
	"fmt"
	"log"
	"net"
	"strings"
	"time"

	"geocoding/countries"
	"geocoding/proto"

	_ "github.com/go-sql-driver/mysql"

	expand "github.com/openvenues/gopostal/expand"

	"geocoding/parser"

	grpc "google.golang.org/grpc"
	"google.golang.org/grpc/reflection"
)

type opengeocodingServer struct {
	database *sql.DB
	proto.UnimplementedOpenGeocodingServer
}

// Make the string SQL safe
func escape_sql(s string) string {
	return strings.ReplaceAll(s, "'", "\\'")
}

func (s *opengeocodingServer) Forward(ctx context.Context, request *proto.ForwardRequest) (*proto.ForwardResponse, error) {

	parsed := parser.ParseAddress(request.Address)

	match := ""
	additionalQuery := ""
	if parsed.Road != "" {
		match += "@street " + escape_sql(parsed.Road) + " "
	}
	if parsed.City != "" {
		match += "@city \"" + escape_sql(parsed.City) + " CPH Cobanhavan Copenaga Copenaghen Copenaguen Copenhaga Copenhagen Copenhague Copenhaguen Copenhaguen Kobenhavn Copenhaguen København Cóbanhávan Hafnia Kapehngagen Kaupmannahoefn Kaupmannahöfn Keypmannahavn Kjobenhavn Kjopenhamn Kjøpenhamn Kobenhamman Kobenhaven Kobenhavn Kodan Kodaň Koebenhavn Koeoepenhamina Koepenhamn Kopenage Kopenchage Kopengagen Kopenhaagen Kopenhag Kopenhaga Kopenhage Kopenhagen Kopenhagena Kopenhago Kopenhāgena Kopenkhagen Koppenhaga Koppenhága Kòpenhaga Köbenhavn Köpenhamn Kööpenhamina København Københámman\"/1 "
	}
	if parsed.Postcode != "" {
		match += "| @(postcode,unit) " + escape_sql(parsed.Postcode) + " "
	}
	if parsed.Unit != "" {
		match += "| @unit " + escape_sql(parsed.Unit) + " "
	}
	if parsed.HouseNumber != "" {
		match += "| @number " + escape_sql(parsed.HouseNumber) + " "
	}
	// need to add country to the table
	if parsed.Country != "" {
		fmt.Println(parsed.Country)
		countryCode := countries.GetCountryCodeFromLabel(parsed.Country)
		fmt.Println(countryCode)
		if countryCode != "" {
			additionalQuery += " AND country_code = '" + countryCode + "'"
		}
	}

	options := expand.GetDefaultExpansionOptions()
	options.Languages = []string{"en"}
	// allAddresses := expand.ExpandAddressOptions(request.Address, options)

	// matches := []string{}
	// for _, address := range allAddresses {
	// 	matches = append(matches, fmt.Sprintf(`"%s"/0.6`, address))
	// }

	// query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM openaddresses WHERE MATCH('@(street,number,unit,city,district,region,postcode) ` + strings.Join(matches, "|") + `') LIMIT 1 OPTION ranker=sph04, field_weights=(street=10,number=2,unit=2,city=4,district=6,region=6,postcode=8)`
	query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM openaddresses WHERE MATCH('` + match + `') ` + additionalQuery + ` LIMIT 1`

	fmt.Println(query)

	rows, err := s.database.Query(query)
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
