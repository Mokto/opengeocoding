package countries

import (
	_ "embed"
	"encoding/json"
	"strings"
)

type Country struct {
	Name   string `json:"name"`
	Native string `json:"native"`
}

// https://raw.githubusercontent.com/annexare/Countries/main/dist/countries.min.json
//
//go:embed countries.json
var jsonFile []byte
var countryLabelToCode map[string]string

func GetCountryCodeFromLabel(countryLabel string) string {
	if countryLabelToCode == nil {
		loadJson()
	}
	return countryLabelToCode[strings.ToLower(countryLabel)]
}

func loadJson() {
	countryLabelToCode = make(map[string]string)
	jsonUnmarshalled := make(map[string]Country)
	json.Unmarshal(jsonFile, &jsonUnmarshalled)

	for key, value := range jsonUnmarshalled {
		loweredKey := strings.ToLower(key)

		countryLabelToCode[strings.ToLower(value.Name)] = loweredKey
		countryLabelToCode[strings.ToLower(value.Native)] = loweredKey
	}
}
