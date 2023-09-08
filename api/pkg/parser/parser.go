package parser

import (
	"geocoding/pkg/geolabels"

	parser "github.com/openvenues/gopostal/parser"
	"golang.org/x/exp/slices"
)

type ParsedAddress struct {
	House         string // Not taken into account
	Category      string // Not taken into account
	Near          string // Not taken into account
	HouseNumber   string
	Road          string
	Unit          string
	Level         string // Not taken into account
	Staircase     string // Not taken into account
	Entrance      string // Not taken into account
	PoBox         string // Not taken into account
	Postcode      string
	Suburb        string // Not taken into account
	CityDistrict  string // Not taken into account
	City          string
	Island        string // Not taken into account
	StateDistrict string // Not taken into account
	State         string // Not taken into account
	CountryRegion string // Not taken into account
	Country       string
	WorldRegion   string // Not taken into account

}

func ParseAddress(address string) ParsedAddress {
	components := parser.ParseAddress(address)

	result := buildAddress(components)

	if result.Country != "" && result.State != "" && result.City != "" {
		countryCode := geolabels.GetCountryCodeFromLabel(result.Country)
		if countryCode == "" {
			countryCode = result.Country
		}
		state := geolabels.GetCountryCodeFromLabel(result.State)
		if state == "" {
			state = result.State
		}
		city := geolabels.GetCountryCodeFromLabel(result.City)
		if city == "" {
			city = result.City
		}
		if state == countryCode {
			result = removeComponentAndRebuildAddress(components, "state", result.State)
		}
		if city == countryCode {
			result = removeComponentAndRebuildAddress(components, "city", result.City)
		}
	}

	return result
}

func removeComponentAndRebuildAddress(components []parser.ParsedComponent, componentToRemove string, value string) ParsedAddress {
	components = slices.DeleteFunc(components, func(c parser.ParsedComponent) bool {
		return c.Label == componentToRemove && c.Value == value
	})
	address := ""
	for _, component := range components {
		address += component.Value + " "
	}
	components = parser.ParseAddress(address)
	return buildAddress(components)
}

func buildAddress(components []parser.ParsedComponent) ParsedAddress {

	result := ParsedAddress{}
	for _, component := range components {
		switch component.Label {
		case "house":
			result.House = component.Value
		case "category":
			result.Category = component.Value
		case "near":
			result.Near = component.Value
		case "house_number":
			result.HouseNumber = component.Value
		case "road":
			if result.Road == "" {
				result.Road = component.Value
			}
		case "unit":
			result.Unit = component.Value
		case "level":
			result.Level = component.Value
		case "staircase":
			result.Staircase = component.Value
		case "entrance":
			result.Entrance = component.Value
		case "po_box":
			result.PoBox = component.Value
		case "postcode":
			result.Postcode = component.Value
		case "suburb":
			result.Suburb = component.Value
		case "city_district":
			result.CityDistrict = component.Value
		case "city":
			result.City = component.Value
		case "island":
			result.Island = component.Value
		case "state_district":
			result.StateDistrict = component.Value
		case "state":
			result.State = component.Value
		case "country_region":
			result.CountryRegion = component.Value
		case "country":
			result.Country = component.Value
		case "world_region":
			result.WorldRegion = component.Value
		default:
			panic("Unknown component " + component.Label)
		}
	}
	return result
}
