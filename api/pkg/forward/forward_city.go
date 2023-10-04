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
)

func forwardCity(container *container.Container, parsed parser.ParsedAddress) (*proto.ForwardResult, error) {

	searchBody := elasticsearch.NewSearchBody()

	showOtherPotentialCities := false

	countryCode := geolabels.GetCountryCodeFromLabel(parsed.Country)
	if countryCode == "" {
		showOtherPotentialCities = true
	} else {
		searchBody.FilterTerm("country_code", countryCode)
	}

	citiesSearchBody := elasticsearch.NewSearchBody()
	for _, city := range parsed.City {
		for _, city := range geolabels.ExpandCityLabel(city) {
			citiesSearchBody.ShouldMatch("city", city)
		}
	}
	citiesSearchBody.MinimumShouldMatch(1)

	searchBody.ShouldCustom(citiesSearchBody)

	if parsed.State != "" {
		searchBody.ShouldMatch("region", parsed.State)
	}

	forwardResult := &proto.ForwardResult{}

	limit := 1
	if showOtherPotentialCities {
		limit = 5
	}

	results, _, err := container.Elasticsearch.SearchMany(context.Background(), "geonames_cities", elasticsearch.SearchParams{
		Body: searchBody.Body(),
		Size: limit,
		Sort: []string{"population:desc"},
	})
	if err != nil {
		return nil, errors.Wrap(err)
	}

	for index, result := range results {
		if index == 0 {
			forwardResult.Location = formatCityResultToLocation(result)
		} else {
			forwardResult.OtherPotentialLocations = append(forwardResult.OtherPotentialLocations, formatCityResultToLocation(result))
		}

	}

	return forwardResult, nil
}

func formatCityResultToLocation(result string) *proto.Location {
	city := gjson.Get(result, "_source.city").String()
	region := gjson.Get(result, "_source.region").String()
	lat := gjson.Get(result, "_source.location.lat").Float()
	long := gjson.Get(result, "_source.location.lon").Float()
	country_code := gjson.Get(result, "_source.country_code").String()
	population := uint32(gjson.Get(result, "_source.population").Int())
	return &proto.Location{
		City:        &city,
		Region:      &region,
		Lat:         float32(lat),
		Long:        float32(long),
		CountryCode: &country_code,
		Source:      proto.Source_Geonames,
		Population:  &population,
	}
}
