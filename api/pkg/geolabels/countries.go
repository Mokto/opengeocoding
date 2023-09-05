package geolabels

import (
	_ "embed"
	"strings"
)

func GetCountryCodeFromLabel(countryLabel string) string {
	return countryLabelToCode[strings.ToLower(countryLabel)]
}
