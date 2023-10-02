package geolabels

import (
	_ "embed"
	"slices"
	"strings"
)

var countryCodes []string

func GetCountryCodeFromLabel(countryLabel string) string {
	if slices.Contains(countryCodes, strings.ToLower(countryLabel)) {
		return strings.ToLower(countryLabel)
	}
	return countryLabelToCode[strings.ToLower(countryLabel)]
}

func loadAllCountryCodes() {
	for code := range countryCodeToLanguages {
		if !slices.Contains(countryCodes, code) {
			countryCodes = append(countryCodes, code)
		}
	}
}
