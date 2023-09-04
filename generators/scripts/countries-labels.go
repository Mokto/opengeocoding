package scripts

import (
	"encoding/json"
	"fmt"
	"generators/download"
	"generators/write"
	"strings"
)

type Country struct {
	Name      string   `json:"name"`
	Native    string   `json:"native"`
	Languages []string `json:"languages"`
}

func GetCountriesData() map[string]Country {
	bytes, err := download.DownloadFile("./data/countries.json", "https://raw.githubusercontent.com/annexare/Countries/main/dist/countries.min.json")
	if err != nil {
		panic(err)
	}

	jsonUnmarshalled := make(map[string]Country)
	err = json.Unmarshal(bytes, &jsonUnmarshalled)
	if err != nil {
		panic(err)
	}

	return jsonUnmarshalled
}

func GenerateCountriesLabels() {

	jsonUnmarshalled := GetCountriesData()

	data := "package geolabels\n\n"
	data += "var countryLabelToCode = map[string]string{\n"
	for key, value := range jsonUnmarshalled {
		loweredKey := strings.ToLower(key)
		data += fmt.Sprintf(`"%s": "%s",`, strings.ToLower(value.Name), loweredKey)
		data += "\n"
		if value.Name != value.Native {
			data += fmt.Sprintf(`"%s": "%s",`, strings.ToLower(value.Native), loweredKey)
			data += "\n"
		}
	}
	data += "}"

	err := write.FormatAndWrite("../api/geolabels/countryLabelToCode.go", data)
	if err != nil {
		panic(err)
	}
}
