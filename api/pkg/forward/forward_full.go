package forward

import (
	"fmt"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"log"
	"strings"
)

func forwardFull(database *manticoresearch.ManticoreSearch, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {
	query := getAddressForwardQuery(parsed, "openaddresses")
	fmt.Println(query)
	if query == "" {
		return &proto.ForwardResult{}, nil
	}

	result, err := runQuery(database, parsed, query)
	if err != nil {
		return nil, err
	}

	if result == nil {
		query := getAddressForwardQuery(parsed, "openstreetdata_houses")
		result, err := runQuery(database, parsed, query)
		if err != nil {
			return nil, err
		}

		if result == nil {
			return &proto.ForwardResult{}, nil
		}

		result.Location.Source = proto.Source_OpenStreetData
		return result, nil
	}

	result.Location.Source = proto.Source_OpenAddresses
	return result, nil
}

func getAddressForwardQuery(parsed parser.ParsedAddress, tableName string) string {

	match := ""
	additionalQuery := ""
	if parsed.Road != nil {
		roads := []string{}
		for _, road := range parsed.Road {
			roads = append(roads, escape_sql(road))
		}
		match += "@street " + strings.Join(roads, " | ") + " "
	} else {
		return ""
	}
	if parsed.City != nil {
		cities := []string{}
		for _, city := range parsed.City {
			for _, city := range geolabels.ExpandCityLabel(city) {
				cities = append(cities, `@city "`+escape_sql(city)+`"`)
			}
		}
		match += "(" + strings.Join(cities, " | ") + " ) "
	}
	if parsed.Postcode != "" || parsed.Unit != "" || parsed.HouseNumber != "" {
		match += " MAYBE ("
		submatch := []string{}
		if parsed.Postcode != "" {
			submatch = append(submatch, "@(postcode,unit) "+escape_sql(parsed.Postcode)+" ")
		}
		if parsed.Unit != "" {
			submatch = append(submatch, "@unit \""+escape_sql(parsed.Unit)+"\"/1 ")
		}
		if parsed.HouseNumber != "" {
			submatch = append(submatch, "@number \""+escape_sql(parsed.HouseNumber)+"\"/1 ")
		}
		match += strings.Join(submatch, " | ")
		match += ")"
	}
	if parsed.Country != "" {
		countryCode := geolabels.GetCountryCodeFromLabel(parsed.Country)
		if countryCode != "" {
			additionalQuery += " AND country_code = '" + countryCode + "'"
		}
	}

	// query := `OPTION ranker=sph04, field_weights=(street=10,number=2,unit=2,city=4,district=6,region=6,postcode=8)`
	query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM ` + tableName + ` WHERE MATCH('` + match + `') ` + additionalQuery + ` LIMIT 1 OPTION field_weights=(street=10,number=4,unit=2,city=9,district=6,region=6,postcode=8)`

	// fmt.Println(query)

	return query
}

func runQuery(database *manticoresearch.ManticoreSearch, parsed parser.ParsedAddress, query string) (*proto.ForwardResult, error) {
	rows, err := database.Balancer.Query(query)
	if err != nil {
		log.Println(err)
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
		if parsed.HouseNumber == "" {
			number = ""
		} else {
			number = parsed.HouseNumber
		}
		if parsed.Unit == "" {
			unit = ""
		}

		return &proto.ForwardResult{
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

	return nil, nil
}
