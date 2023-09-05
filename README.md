# Open geocoding

Based on 3 sub-projects:

- api: golang based tool that start a GRPC & HTTP server to server forward geocoding requests
- importers: rust based fast importers of open source data (for now just openaddresses)
- generators: useful to generate necessary mapping for the API. For example a JSON file that mapping a city name to all its language variations

## Dev Dependencies

- Gow for reload the golang server when changing any file (go install github.com/mitranim/gow@latest)
- Buf to generate protobuf definitions

## Generate protobuf definitions

$ buf generate .


## API

$ go run main.go

$ gow run main.go # watch mode 




CREATE TABLE IF NOT EXISTS openaddresses(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'

ALTER CLUSTER manticore_cluster ADD openaddresses


CREATE TABLE IF NOT EXISTS geonames_cities(city text, region text, lat float, long float, country_code string, population int)  rt_mem_limit = '1G'

ALTER CLUSTER manticore_cluster ADD geonames_cities
