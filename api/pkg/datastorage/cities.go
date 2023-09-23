package datastorage

import (
	"fmt"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/proto"
	"strings"
)

var citiesTableName = "geonames_cities"

func (datastorage *Datastorage) initCities() {

	_, err := datastorage.database.Worker.Exec("CREATE TABLE IF NOT EXISTS " + citiesTableName + "(city text, region text, lat float, long float, country_code string, population int) rt_mem_limit = '1G'")
	if err != nil {
		panic(err)
	}

	datastorage.database.Worker.Exec("ALTER CLUSTER opengeocoding_cluster ADD " + citiesTableName)
	// _, err = datastorage.database.Worker.Exec("ALTER CLUSTER opengeocoding_cluster ADD " + citiesTableName)
	// if err != nil {
	// 	panic(err)
	// }

}

func (datastorage *Datastorage) InsertCities(locations []*proto.Location) error {
	values := []string{}

	for _, location := range locations {
		values = append(values, fmt.Sprintf(
			"(%d, '%s', '%s', %f, %f, '%s', %d)",
			manticoresearch.HashString(*location.Id),
			manticoresearch.CleanString(*location.City),
			manticoresearch.CleanString(*location.Region),
			location.Lat,
			location.Long,
			*location.CountryCode,
			*location.Population,
		))
	}

	query := "REPLACE INTO opengeocoding_cluster:" + citiesTableName + "(id,city,region,lat,long,country_code,population) VALUES " + strings.Join(values, ",")

	_, err := datastorage.database.Worker.Exec(query)
	if err != nil {
		fmt.Println(query)
		return err
	}
	return nil

}
