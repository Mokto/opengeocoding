package geolabels

func GetCountryLanguages(country_code string) []string {
	return countryCodeToLanguages[country_code]
}
