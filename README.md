# Open geocoding

Based on 3 sub-projects:

- API: golang based tool that start a GRPC & HTTP to serve forward geocoding requests
- Importers: super fast Rust based importers of open source data
    - Openaddresses for ... addresses
    - [Openstreetdata](https://openstreetdata.org/) to get addresses & streets
    - Geonames for cities
    - Who's on first to attribute locations to regions & countries
- Generators: useful to generate necessary mapping for the API. For example a JSON file that mapping a city name to all its language variations

## Dev Dependencies

- Gow to reload the golang server when changing any file (go install github.com/mitranim/gow@latest)
- Buf to generate protobuf definitions

## Generate protobuf definitions

$ buf generate .


## API

```
$ go run main.go

$ gow run main.go # watch mode 
```


## Importers


```
$ cargo run --bin openstreetdata_import

$ cargo run --bin openaddress_import

$ cargo run --bin geonames_import
```