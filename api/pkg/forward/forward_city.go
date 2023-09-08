package forward

import (
	"database/sql"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"
	"log"
	"strings"
)

func forwardCity(database *sql.DB, parsed parser.ParsedAddress) (*proto.Location, error) {

	countryCode := geolabels.GetCountryCodeFromLabel(parsed.Country)
	if countryCode == "" {
		return nil, nil
	}
	cities := []string{}
	cities_exact := []string{}
	for _, city := range geolabels.ExpandCityLabel(parsed.City) {
		cities = append(cities, escape_sql(city))
		cities_exact = append(cities_exact, "^"+escape_sql(city)+"$")
	}
	additionalQuery := ""
	if parsed.State != "" {
		additionalQuery = " @region " + escape_sql(parsed.State)
	}

	query := `SELECT city, region, lat, long, country_code FROM geonames_cities WHERE MATCH('@city "` + strings.Join(cities, " ") + `"/1 | ` + strings.Join(cities_exact, ` | `) + additionalQuery + `') AND country_code = '` + countryCode + `' ORDER BY weight() DESC, population DESC LIMIT 1 OPTION ranker=wordcount`

	rows, err := database.Query(query)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

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
		return &proto.Location{
			City:        &city,
			Region:      &region,
			Lat:         &lat,
			Long:        &long,
			CountryCode: &country_code,
		}, nil
	}

	return nil, nil
}
