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
			cities = append(cities, `@city "`+escape_sql(city)+`"`)
			cities_exact = append(cities_exact, `"^`+escape_sql(city)+`$"`)
		}
	}
	additionalQuery := ""
	if parsed.State != "" {
		additionalQuery = ` MAYBE @region "` + escape_sql(parsed.State) + `"/1`
	}

	limit := 1
	if showOtherPotentialCities {
		limit = 5
	}
	query := `SELECT (weight() + population / 1000) as score, city, region, lat, long, country_code FROM geonames_cities WHERE MATCH('(` + strings.Join(cities, " | ") + `) | ` + strings.Join(cities_exact, ` | `) + additionalQuery + `') ` + country_query + ` ORDER BY score DESC LIMIT ` + strconv.Itoa(limit) + ` OPTION max_predicted_time=10000, max_matches=` + strconv.Itoa(limit)
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
			score        float32
			city         string
			region       string
			lat          float32
			long         float32
			country_code string
		)
		if err := rows.Scan(&score, &city, &region, &lat, &long, &country_code); err != nil {
			log.Fatal(err)
		}

		location := &proto.Location{
			City:        &city,
			Region:      &region,
			Lat:         &lat,
			Long:        &long,
			CountryCode: &country_code,
			Source:      proto.Source_Geonames,
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
