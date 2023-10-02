package parser

import (
	expand "github.com/openvenues/gopostal/expand"
)

func ExpandAddress(address string, languages []string) []string {

	options := expand.GetDefaultExpansionOptions()
	options.Languages = languages
	return expand.ExpandAddressOptions(address, options)

}
