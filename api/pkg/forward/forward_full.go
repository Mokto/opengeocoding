package forward

import (
	"database/sql"
	"fmt"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"log"
	"strings"
)

func forwardFull(database *sql.DB, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {

	match := ""
	additionalQuery := ""
	if parsed.Road != "" {
		match += "@street " + escape_sql(parsed.Road) + " "
	} else {
		return nil, nil
	}
	if parsed.City != "" {
		cities := []string{}
		for _, city := range geolabels.ExpandCityLabel(parsed.City) {
			cities = append(cities, escape_sql(city))
		}
		match += "@city \"" + strings.Join(cities, " ") + " \"/1 "
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
	query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM openaddresses WHERE MATCH('` + match + `') ` + additionalQuery + ` LIMIT 1 OPTION field_weights=(street=10,number=4,unit=2,city=9,district=6,region=6,postcode=8)`

	fmt.Println(query)
	rows, err := database.Query(query)
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
		if parsed.HouseNumber == "" {
			number = ""
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

	return &proto.ForwardResult{}, nil
}
