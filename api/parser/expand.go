package parser

import (
	expand "github.com/openvenues/gopostal/expand"
)

func ExpandAddress(address string) []string {

	options := expand.GetDefaultExpansionOptions()
	options.Languages = []string{"en"}
	return expand.ExpandAddressOptions(address, options)

}
