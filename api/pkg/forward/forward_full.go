package forward

import (
	"context"
	"geocoding/pkg/container"
	"geocoding/pkg/elasticsearch"
	"geocoding/pkg/errors"
	"geocoding/pkg/geolabels"
	"geocoding/pkg/parser"
	"geocoding/pkg/proto"

	"github.com/tidwall/gjson"
	"golang.org/x/exp/slices"
)

func forwardFull(container *container.Container, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {
	query := getAddressForwardQuery(parsed)
	if query == nil {
		return &proto.ForwardResult{}, nil
	}

	result, err := runQuery(container, parsed, query, "openaddresses")
	if err != nil {
		return nil, err
	}

	if result == nil {
		return &proto.ForwardResult{}, nil
		// result, err := runQuery(container, parsed, query, "openstreetdata_addresses")
		// if err != nil {
		// 	return nil, err
		// }

		// if result == nil {
		// 	return &proto.ForwardResult{}, nil
		// }

		// result.Location.Source = proto.Source_OpenStreetDataAddress
		// return result, nil
	}

	result.Location.Source = proto.Source_OpenAddresses

	return result, nil
}

func getAddressForwardQuery(parsed parser.ParsedAddress) *elasticsearch.SearchBody {

	searchBody := elasticsearch.NewSearchBody()

	countryCode := ""
	languages := []string{}
	if parsed.Country != "" {
		countryCode = geolabels.GetCountryCodeFromLabel(parsed.Country)
		searchBody.FilterTerm("country_code", countryCode)
		languages = geolabels.GetCountryLanguages(countryCode)
	}

	if parsed.Road != nil {
		roadsSearchBody := elasticsearch.NewSearchBody()
		for _, road := range parsed.Road {
			expandedRoads := parser.ExpandAddress(road, languages)
			if !slices.Contains(expandedRoads, road) {
				expandedRoads = append(expandedRoads, road)
			}
			for _, road := range expandedRoads {
				roadsSearchBody.ShouldMatch("street", road)
			}
		}
		roadsSearchBody.MinimumShouldMatch(1)
		searchBody.ShouldCustom(roadsSearchBody)
	} else {
		return nil
	}
	if parsed.City != nil {
		citiesSearchBody := elasticsearch.NewSearchBody()
		for _, city := range parsed.City {
			for _, city := range geolabels.ExpandCityLabel(city) {
				citiesSearchBody.ShouldMatch("city", city)
			}
		}
		citiesSearchBody.MinimumShouldMatch(1)
		searchBody.ShouldCustom(citiesSearchBody)
	}
	if parsed.Postcode != "" || parsed.Unit != "" || parsed.HouseNumber != "" || parsed.State != "" {
		additionalSearchBody := elasticsearch.NewSearchBody()
		if parsed.Postcode != "" {
			additionalSearchBody.ShouldMatch("postcode", parsed.Postcode)
			additionalSearchBody.ShouldMatch("unit", parsed.Postcode)
		}
		if parsed.Unit != "" {
			additionalSearchBody.ShouldMatch("unit", parsed.Unit)
		}
		if parsed.HouseNumber != "" {
			additionalSearchBody.ShouldMatch("number", parsed.HouseNumber)
		}
		if parsed.State != "" {
			additionalSearchBody.ShouldMatch("region", parsed.State)
		}
		additionalSearchBody.MinimumShouldMatch(0)
	}

	searchBody.MinimumShouldMatch("100%")

	searchBody.Debug()
	return searchBody
}

func runQuery(container *container.Container, parsed parser.ParsedAddress, searchBody *elasticsearch.SearchBody, tableName string) (*proto.ForwardResult, error) {

	result, err := container.Elasticsearch.SearchOne(context.Background(), tableName, elasticsearch.SearchParams{
		Body: searchBody.Body(),
		Size: 1,
	})
	if err != nil {
		return nil, errors.Wrap(err)
	}
	if result == "" {
		return nil, nil
	}

	location := formatAddressResultToLocation(result, parsed)

	return &proto.ForwardResult{
		Location: location,
	}, nil

}

func formatAddressResultToLocation(result string, parsed parser.ParsedAddress) *proto.Location {
	city := gjson.Get(result, "_source.city").String()
	region := gjson.Get(result, "_source.region").String()
	lat := gjson.Get(result, "_source.location.lat").Float()
	long := gjson.Get(result, "_source.location.lon").Float()
	country_code := gjson.Get(result, "_source.country_code").String()
	street := gjson.Get(result, "_source.street").String()
	number := gjson.Get(result, "_source.number").String()
	unit := gjson.Get(result, "_source.unit").String()
	postcode := gjson.Get(result, "_source.postcode").String()
	district := gjson.Get(result, "_source.district").String()
	location := &proto.Location{
		City:        &city,
		Region:      &region,
		Lat:         float32(lat),
		Long:        float32(long),
		CountryCode: &country_code,
		Source:      proto.Source_Geonames,
		Street:      &street,
		Number:      &number,
		Unit:        &unit,
		Postcode:    &postcode,
		District:    &district,
	}
	emptyString := ""

	if parsed.HouseNumber == "" {
		location.Number = &emptyString
	} else {
		location.Number = &parsed.HouseNumber
	}
	if parsed.Unit == "" {
		location.Unit = &emptyString
	}
	computeFullStreetAddress(location)

	return location
}

func computeFullStreetAddress(location *proto.Location) {
	if location.Number == nil || *location.Number == "" {
		location.FullStreetAddress = location.Street
		return
	}

	if location.CountryCode != nil && *location.CountryCode != "be" && (slices.Contains(geolabels.GetCountryLanguages(*location.CountryCode), "en") || slices.Contains(geolabels.GetCountryLanguages(*location.CountryCode), "fr")) {
		address := *location.Number + " " + *location.Street
		location.FullStreetAddress = &address
	} else {
		address := *location.Street + " " + *location.Number
		location.FullStreetAddress = &address
	}
	if location.Unit != nil && *location.Unit != "" {
		address := *location.FullStreetAddress + " " + *location.Unit
		location.FullStreetAddress = &address
	}

}
