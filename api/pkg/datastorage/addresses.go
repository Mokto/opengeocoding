package datastorage

import (
	"fmt"
	"geocoding/pkg/manticoresearch"
	"geocoding/pkg/proto"
	"strings"
)

var openaddressesTableName = "openaddresses"
var openstreetdataAddressesTableName = "openstreetdata_addresses"

func (datastorage *Datastorage) initOpenAddresses() {
	datastorage.initAddressesTable(openaddressesTableName)
}

func (datastorage *Datastorage) initOpenstreetdataAddresses() {
	datastorage.initAddressesTable(openstreetdataAddressesTableName)
}

func (datastorage *Datastorage) initAddressesTable(tableName string) {
	_, err := datastorage.database.Worker.Exec("CREATE TABLE IF NOT EXISTS " + tableName + "(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'")
	if err != nil {
		panic(err)
	}

	datastorage.database.Worker.Exec("ALTER CLUSTER opengeocoding_cluster ADD " + tableName)
	// _, err = datastorage.database.Worker.Exec("ALTER CLUSTER opengeocoding_cluster ADD " + openaddressesTableName)
	// if err != nil {
	// 	panic(err)
	// }
}

func (datastorage *Datastorage) InsertAddresses(locations []*proto.Location, source proto.Source) error {
	tableName := ""
	if source == proto.Source_OpenStreetDataAddress {
		tableName = openstreetdataAddressesTableName
	} else if source == proto.Source_OpenAddresses {
		tableName = openaddressesTableName
	} else {
		return fmt.Errorf("source not supported for addresses %s", source)
	}

	values := []string{}

	for _, location := range locations {
		values = append(values, fmt.Sprintf(
			"(%d,'%s','%s','%s','%s','%s','%s','%s',%f,%f, '%s')",
			manticoresearch.HashString(*location.Id),
			manticoresearch.CleanString(*location.Street),
			manticoresearch.CleanString(*location.Number),
			manticoresearch.CleanString(*location.Unit),
			manticoresearch.CleanString(*location.City),
			manticoresearch.CleanString(*location.District),
			manticoresearch.CleanString(*location.Region),
			manticoresearch.CleanString(*location.Postcode),
			location.Lat,
			location.Long,
			*location.CountryCode,
		))
	}

	query := "REPLACE INTO opengeocoding_cluster:" + tableName + "(id,street,number,unit,city,district,region,postcode,lat,long,country_code) VALUES " + strings.Join(values, ",")

	_, err := datastorage.database.Worker.Exec(query)
	if err != nil {
		return err
	}
	return nil

}
