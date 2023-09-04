package forward

import (
	"database/sql"
	"fmt"
	"geocoding/countries"
	"geocoding/parser"
	"geocoding/proto"
	"log"
	"strings"

	expand "github.com/openvenues/gopostal/expand"
)

// Make the string SQL safe
func escape_sql(s string) string {
	return strings.ReplaceAll(s, "'", "\\'")
}

func Forward(database *sql.DB, address string) (*proto.Location, error) {
	parsed := parser.ParseAddress(address)

	match := ""
	additionalQuery := ""
	if parsed.Road != "" {
		match += "@street " + escape_sql(parsed.Road) + " "
	}
	if parsed.City != "" {
		match += "@city \"" + escape_sql(parsed.City) + " CPH Cobanhavan Copenaga Copenaghen Copenaguen Copenhaga Copenhagen Copenhague Copenhaguen Copenhaguen Kobenhavn Copenhaguen København Cóbanhávan Hafnia Kapehngagen Kaupmannahoefn Kaupmannahöfn Keypmannahavn Kjobenhavn Kjopenhamn Kjøpenhamn Kobenhamman Kobenhaven Kobenhavn Kodan Kodaň Koebenhavn Koeoepenhamina Koepenhamn Kopenage Kopenchage Kopengagen Kopenhaagen Kopenhag Kopenhaga Kopenhage Kopenhagen Kopenhagena Kopenhago Kopenhāgena Kopenkhagen Koppenhaga Koppenhága Kòpenhaga Köbenhavn Köpenhamn Kööpenhamina København Københámman\"/1 "
	}
	if parsed.Postcode != "" || parsed.Unit != "" || parsed.HouseNumber != "" {
		match += " MAYBE ("
		submatch := []string{}
		if parsed.Postcode != "" {
			submatch = append(submatch, " @(postcode,unit) "+escape_sql(parsed.Postcode)+" ")
		}
		if parsed.Unit != "" {
			submatch = append(submatch, " @unit "+escape_sql(parsed.Unit)+" ")
		}
		if parsed.HouseNumber != "" {
			submatch = append(submatch, " @number "+escape_sql(parsed.HouseNumber)+" ")
		}
		match += strings.Join(submatch, " | ")
		match += ")"
	}
	if parsed.Country != "" {
		countryCode := countries.GetCountryCodeFromLabel(parsed.Country)
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
	query := `SELECT street, number, unit, city, district, region, postcode, lat, long, country_code FROM openaddresses WHERE MATCH('` + match + `') ` + additionalQuery + ` LIMIT 1 OPTION field_weights=(street=10,number=2,unit=2,city=4,district=6,region=6,postcode=8)`

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
		return &proto.Location{
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
		}, nil
	}

	return nil, nil
}
