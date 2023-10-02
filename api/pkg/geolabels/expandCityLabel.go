package geolabels

import (
	"strings"
)

var cityLabels map[string][]string

func ExpandCityLabel(cityLabel string) []string {
	if cityLabels == nil {
		panic("cityLabels not initialized. Please call geolabels.BuildCityLabels()")
	}
	result := cityLabels[strings.ToLower(cityLabel)]
	if result == nil {
		return []string{strings.ToLower(cityLabel)}
	}

	return result
}

func BuildCityLabels() {
	cityLabels = map[string][]string{}
	for _, group := range citiesLabelGroups {
		for _, city := range group {
			cityLabels[city] = group
		}
	}
}
