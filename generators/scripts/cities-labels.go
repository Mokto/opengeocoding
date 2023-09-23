package scripts

import (
	"fmt"
	"generators/download"
	"generators/write"
	"log"
	"strings"

	"golang.org/x/exp/slices"
)

func GenerateCitiesLabels() {

	countriesData := GetCountriesData()

	bytesCities, err := download.DownloadAndUnzip("./data/cities15000.zip", "https://download.geonames.org/export/dump/cities15000.zip", "./data/cities15000.txt", "cities15000.txt")
	if err != nil {
		panic(err)
	}

	bytesAlternateNames, err := download.DownloadAndUnzip("./data/alternateNamesV2.zip", "https://download.geonames.org/export/dump/alternateNamesV2.zip", "./data/alternateNamesV2.txt", "alternateNamesV2.txt")
	if err != nil {
		panic(err)
	}

	records15000, err := download.ReadCsv(bytesCities)
	if err != nil {
		log.Fatal(err)
	}
	allRecords, err := download.ReadCsv(bytesAlternateNames)
	if err != nil {
		log.Fatal(err)
	}

	// records15000 = records15000[:20] // Copenhagen

	allCities := [][]string{}

	for index, record := range records15000 {
		if index%50 == 0 {
			fmt.Println("Doing record", index, "of", len(records15000))
		}
		geonameId := record[0]
		geonameName := record[1]
		country := record[8]
		languages := countriesData[country].Languages
		cities := []string{strings.ToLower(geonameName)}

		for _, record := range allRecords {
			if (record[1] == geonameId) && slices.Contains(languages, record[2]) {
				if !slices.Contains(cities, strings.ToLower(record[3])) {
					cities = append(cities, strings.ToLower(record[3]))
				}
			}
		}

		if len(cities) > 1 {
			allCities = append(allCities, cities)
		}
	}

	data := "package geolabels\n\n"
	data += "var citiesLabelGroups = [][]string{\n"
	for _, cities := range allCities {
		data += `{"` + strings.Join(cities, `","`) + `"}`
		data += ",\n"
	}
	data += `{"nyc", "new york city", "new york", "the big apple"}`
	data += `{"mexico city", "ciudad de méxico", "méxico distrito federal", "ciudad de méjico", "méxico", "valle de méxico", "cdmx"},`
	data += "}"

	fmt.Println(data)

	err = write.FormatAndWrite("../api/pkg/geolabels/citiesLabelGroups.go", data)
	if err != nil {
		panic(err)
	}
}
