# Open geocoding

## Dev Dependencies

- Gow for reload the golang server when changing any file (go install github.com/mitranim/gow@latest)
- Buf to generate protobuf definitions

## Generate protobuf definitions

$ buf generate .


## API

$ go run main.go

$ gow run main.go # watch mode 




CREATE TABLE IF NOT EXISTS openaddresses(street text, number text, unit text, city text, district text, region text, postcode text, lat float, long float, country_code string)  rt_mem_limit = '1G'

ALTER CLUSTER manticore_cluster ADD openaddresses # until it succeeds
