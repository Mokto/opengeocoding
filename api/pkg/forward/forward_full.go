package forward

import (
	"fmt"
	"geocoding/pkg/container"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"log"
	"strings"

	"golang.org/x/exp/slices"
)

func forwardFull(container *container.Container, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {
	query := getAddressForwardQuery(parsed, "openaddresses")
	fmt.Println(query)
	if query == "" {
		return &proto.ForwardResult{}, nil
	}

	result, err := runQuery(container, parsed, query)
	if err != nil {
		return nil, err
	}

	if result == nil {
		query := getAddressForwardQuery(parsed, "openstreetdata_addresses")
		result, err := runQuery(container, parsed, query)
		if err != nil {
			return nil, err
		}

		if result == nil {
			return &proto.ForwardResult{}, nil
		}

		appendFullStreetAddres(result)
		result.Location.Source = proto.Source_OpenStreetDataAddress
		return result, nil
	}

	appendFullStreetAddres(result)
	result.Location.Source = proto.Source_OpenAddresses

	return result, nil
}

func appendFullStreetAddres(result *proto.ForwardResult) {
	if result.Location.Number == nil || *result.Location.Number == "" {
		result.Location.FullStreetAddress = result.Location.Street
		return
	}

	if result.Location.CountryCode != nil && *result.Location.CountryCode != "be" && (slices.Contains(geolabels.GetCountryLanguages(*result.Location.CountryCode), "en") || slices.Contains(geolabels.GetCountryLanguages(*result.Location.CountryCode), "fr")) {
		address := *result.Location.Number + " " + *result.Location.Street
		result.Location.FullStreetAddress = &address
	} else {
		address := *result.Location.Street + " " + *result.Location.Number
		result.Location.FullStreetAddress = &address
	}
	if result.Location.Unit != nil && *result.Location.Unit != "" {
		address := *result.Location.FullStreetAddress + " " + *result.Location.Unit
		result.Location.FullStreetAddress = &address
	}
}

func getAddressForwardQuery(parsed parser.ParsedAddress, tableName string) string {

	match := ""
	additionalQuery := ""
	if parsed.Road != nil {
		roads := []string{}
		for _, road := range parsed.Road {
			expandedRoads := parser.ExpandAddress(road)
			if !slices.Contains(expandedRoads, road) {
				expandedRoads = append(expandedRoads, road)
			}
			for _, road := range expandedRoads {
				roads = append(roads, `@street "`+escape_sql(road)+`"`)
			}
		}
		match += "(" + strings.Join(roads, " | ") + ") "
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
	if parsed.Postcode != "" || parsed.Unit != "" || parsed.HouseNumber != "" || parsed.State != "" {
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
		if parsed.State != "" {
			submatch = append(submatch, `@region "`+escape_sql(parsed.State)+`" `)
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
	query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM ` + tableName + ` WHERE MATCH('` + match + `') ` + additionalQuery + ` LIMIT 1 OPTION field_weights=(street=10,number=4,unit=2,city=9,district=6,region=6,postcode=8), max_predicted_time=10000, max_matches=1`

	// fmt.Println(query)

	return query
}

func runQuery(container *container.Container, parsed parser.ParsedAddress, query string) (*proto.ForwardResult, error) {
	rows, err := container.Database.Balancer.Query(query)
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
				Lat:         lat,
				Long:        long,
				CountryCode: &country_code,
			},
		}, nil
	}

	return nil, nil
}
