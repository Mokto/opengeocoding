package scripts

import (
	"fmt"
	"generators/write"
	"strings"
)

func GenerateCountriesLanguages() {

	jsonUnmarshalled := GetCountriesData()

	data := "package geolabels\n\n"
	data += "var countryCodeToLanguages = map[string][]string{\n"
	for key, value := range jsonUnmarshalled {
		countryCode := strings.ToLower(key)
		languages := []string{}
		for _, language := range value.Languages {
			languages = append(languages, strings.ToLower(language))
		}
		data += fmt.Sprintf(`"%s": {"%s"},`, countryCode, strings.Join(languages, `","`))
		data += "\n"
	}
	data += "}"

	err := write.FormatAndWrite("../api/pkg/geolabels/countryCodeToLanguages.go", data)
	if err != nil {
		panic(err)
	}
}
