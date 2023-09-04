package parser

import (
	parser "github.com/openvenues/gopostal/parser"
)

type ParsedAddress struct {
	Postcode    string
	City        string
	Road        string
	HouseNumber string
	Unit        string
	Country     string
}

func ParseAddress(address string) ParsedAddress {
	parsed := parser.ParseAddress(address)

	result := ParsedAddress{}
	for _, component := range parsed {
		if component.Label == "postcode" {
			result.Postcode = component.Value
		} else if component.Label == "city" {
			result.City = component.Value
		} else if component.Label == "road" {
			if result.Road == "" {
				result.Road = component.Value
			}
		} else if component.Label == "house_number" {
			result.HouseNumber = component.Value
		} else if component.Label == "unit" {
			result.Unit = component.Value
		} else if component.Label == "country" {
			result.Country = component.Value
		} else {
			panic("Unknown component " + component.Label)
		}
	}

	return result
}
