package forward

import (
	"fmt"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"log"
	"strconv"
	"strings"
)

func forwardCity(database *manticoresearch.ManticoreSearch, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {

	showOtherPotentialCities := false

	countryCode := geolabels.GetCountryCodeFromLabel(parsed.Country)
	country_query := ""
	if countryCode == "" {
		showOtherPotentialCities = true
		// return nil, nil
	} else {
		country_query = " AND country_code = '" + countryCode + "'"
	}
	cities := []string{}
	cities_exact := []string{}
	for _, city := range parsed.City {
		for _, city := range geolabels.ExpandCityLabel(city) {
			cities = append(cities, escape_sql(city))
			cities_exact = append(cities_exact, "^"+escape_sql(city)+"$")
		}
	}
	additionalQuery := ""
	if parsed.State != "" {
		additionalQuery = " @region " + escape_sql(parsed.State)
	}

	limit := 1
	if showOtherPotentialCities {
		limit = 5
	}
	query := `SELECT city, region, lat, long, country_code FROM geonames_cities WHERE MATCH('@city "` + strings.Join(cities, " ") + `"/1 | ` + strings.Join(cities_exact, ` | `) + additionalQuery + `') ` + country_query + ` ORDER BY weight() DESC, population DESC LIMIT ` + strconv.Itoa(limit) + ` OPTION ranker=wordcount`
	fmt.Println(query)

	rows, err := database.Balancer.Query(query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	result := &proto.ForwardResult{}
	index := 0
	for rows.Next() {
		var (
			city         string
			region       string
			lat          float32
			long         float32
			country_code string
		)
		if err := rows.Scan(&city, &region, &lat, &long, &country_code); err != nil {
			log.Fatal(err)
		}

		location := &proto.Location{
			City:        &city,
			Region:      &region,
			Lat:         &lat,
			Long:        &long,
			CountryCode: &country_code,
		}

		if index == 0 {

			result.Location = location
		} else {
			result.OtherPotentialLocations = append(result.OtherPotentialLocations, location)
		}

		index++
	}

	return result, nil
}
