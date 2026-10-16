# Changelog

## [0.7.2](https://github.com/Mokto/opengeocoding/compare/v0.7.1...v0.7.2) (2026-10-16)


### Bug Fixes

* hand a build Job an earlier leader created over to Argo CD ([#120](https://github.com/Mokto/opengeocoding/issues/120)) ([1513604](https://github.com/Mokto/opengeocoding/commit/1513604299c1da6fe41dc63d72c00276411635a2))
* nest the build Job and serving Deployment under the workloads ConfigMap in Argo CD ([#122](https://github.com/Mokto/opengeocoding/issues/122)) ([ad34892](https://github.com/Mokto/opengeocoding/commit/ad348928317046740f6b8fe20dcec3630126aa42))

## [0.7.1](https://github.com/Mokto/opengeocoding/compare/v0.7.0...v0.7.1) (2026-10-16)


### Bug Fixes

* **chart:** load the build Job's libpostal models from libpostal's data dir ([#119](https://github.com/Mokto/opengeocoding/issues/119)) ([93a5034](https://github.com/Mokto/opengeocoding/commit/93a5034243bcc266d5e5a909eb0ca8bdc05d3b10))
* show the leader's build Job and serving Deployment as top-level Argo CD resources ([#116](https://github.com/Mokto/opengeocoding/issues/116)) ([d7fb236](https://github.com/Mokto/opengeocoding/commit/d7fb236197b9b3835061cd541b6145ff602b3ad8))
* stop the evaluated service where no kill binary is installed ([#118](https://github.com/Mokto/opengeocoding/issues/118)) ([c201cfa](https://github.com/Mokto/opengeocoding/commit/c201cfaffe4153c859a3e21da61df7a59ff327a7))

## [0.7.0](https://github.com/Mokto/opengeocoding/compare/v0.6.0...v0.7.0) (2026-10-16)


### Features

* add GeoNames populated places where Overture has none nearby ([#105](https://github.com/Mokto/opengeocoding/issues/105)) ([0a865e6](https://github.com/Mokto/opengeocoding/commit/0a865e666ba311904d0576237dfba8916b7b2542))
* **chart:** build the index and roll out serving from an in-cluster leader ([#99](https://github.com/Mokto/opengeocoding/issues/99)) ([edb90b9](https://github.com/Mokto/opengeocoding/commit/edb90b99bdbbe2603dc698d0e5e19d81d638c899))
* number the build log's phases and report transfer progress ([#100](https://github.com/Mokto/opengeocoding/issues/100)) ([aab0cd2](https://github.com/Mokto/opengeocoding/commit/aab0cd21dbf538712c2cc349cd8af78b1d22b745))


### Bug Fixes

* build the leader with the progress module that store.rs now needs ([#107](https://github.com/Mokto/opengeocoding/issues/107)) ([89ab9b9](https://github.com/Mokto/opengeocoding/commit/89ab9b977a323ff5d0976f2512c1162b6117ddaa))
* **chart:** keep the build Job's libpostal models on the scratch volume ([#97](https://github.com/Mokto/opengeocoding/issues/97)) ([a528181](https://github.com/Mokto/opengeocoding/commit/a528181c044ee64f6d69540ec64256ce63091c1d))
* hide unchanged evaluation check tables ([#113](https://github.com/Mokto/opengeocoding/issues/113)) ([49e66f3](https://github.com/Mokto/opengeocoding/commit/49e66f376cb780ab145ad9515049155f9555832c))
* ResolveRegion near borders and add county subdivision codes (ISO3166-2-lvl6) ([#104](https://github.com/Mokto/opengeocoding/issues/104)) ([b4509c9](https://github.com/Mokto/opengeocoding/commit/b4509c974e4c9a70865e3d52b775608aa237e1bc))


### Performance Improvements

* run the evaluator's local service without a rate limit or budget ([#93](https://github.com/Mokto/opengeocoding/issues/93)) ([eabf68f](https://github.com/Mokto/opengeocoding/commit/eabf68f624b6803f3923be15b1287e92ade2c23e))
* speed up the OSM build and house-number phase with decode-once and 64 shards ([#106](https://github.com/Mokto/opengeocoding/issues/106)) ([9e52127](https://github.com/Mokto/opengeocoding/commit/9e52127833b8b98b338502dc1a2fa4ac9e134090))

## [0.6.0](https://github.com/Mokto/opengeocoding/compare/v0.5.2...v0.6.0) (2026-10-16)


### Features

* make index builds reproducible and read reviewed OpenAddresses sources with no stated licence ([#85](https://github.com/Mokto/opengeocoding/issues/85)) ([c6fdc6b](https://github.com/Mokto/opengeocoding/commit/c6fdc6b3c34e5c3fe8b9aee8cde5175200093608))


### Bug Fixes

* answer queries with repeated parts, stated countries and stated regions ([#92](https://github.com/Mokto/opengeocoding/issues/92)) ([d845804](https://github.com/Mokto/opengeocoding/commit/d845804209e7673022aae876e0fa26ed3b375467))

## [0.5.2](https://github.com/Mokto/opengeocoding/compare/v0.5.1...v0.5.2) (2026-10-15)


### Bug Fixes

* repair SuggestAddress defects found by a 208-case gold set ([#84](https://github.com/Mokto/opengeocoding/issues/84)) ([8c22fd6](https://github.com/Mokto/opengeocoding/commit/8c22fd641a04e003d74cd387ee900a7b3c79a91a))
* resolve more US addresses and score ZIP+4 as its ZIP ([#81](https://github.com/Mokto/opengeocoding/issues/81)) ([2e01ca0](https://github.com/Mokto/opengeocoding/commit/2e01ca031b1430d7d55647f531a56508c7f4d67e))

## [0.5.1](https://github.com/Mokto/opengeocoding/compare/v0.5.0...v0.5.1) (2026-10-15)


### Bug Fixes

* answer the house when the stated town contradicts a matching number, road and postcode ([#75](https://github.com/Mokto/opengeocoding/issues/75)) ([a69b921](https://github.com/Mokto/opengeocoding/commit/a69b92193aa9bad431ac562b58d2458140f28510))
* match US addresses whose USPS town differs from the index's municipality ([#71](https://github.com/Mokto/opengeocoding/issues/71)) ([7fbd787](https://github.com/Mokto/opengeocoding/commit/7fbd7878b992db22f671ac8a2a0c303c5b562f91))


### Performance Improvements

* seek each kind's document runs, filter countries from a table and remember a request's lookups ([#80](https://github.com/Mokto/opengeocoding/issues/80)) ([6c1c595](https://github.com/Mokto/opengeocoding/commit/6c1c595a991ced17ef19d95ec3c2e9db033d7398))

## [0.5.0](https://github.com/Mokto/opengeocoding/compare/v0.4.0...v0.5.0) (2026-10-14)


### Features

* add ReverseGeocode RPC returning the nearest houses to a point ([#53](https://github.com/Mokto/opengeocoding/issues/53)) ([d22cc31](https://github.com/Mokto/opengeocoding/commit/d22cc3134922b7bf5d5686a8f4ea723882ab73c5))


### Bug Fixes

* return the stated town when a postcode blocks a match, and stop wrong-country matches ([#60](https://github.com/Mokto/opengeocoding/issues/60)) ([f7c4ce4](https://github.com/Mokto/opengeocoding/commit/f7c4ce45683215ff599050f8d2365947214cb423))


### Performance Improvements

* cut SuggestAddress tail latency (memoized labels, country search, work bounds, warm-up) ([#49](https://github.com/Mokto/opengeocoding/issues/49)) ([c6ba9c9](https://github.com/Mokto/opengeocoding/commit/c6ba9c9ed6a7f17a424d6d3817bb24a2e1039646))
* find the nearest locality from a startup grid, 35 ms to 0.2 ms ([#59](https://github.com/Mokto/opengeocoding/issues/59)) ([d3d5e8c](https://github.com/Mokto/opengeocoding/commit/d3d5e8c79849141d8ed505981ca80f8245f5add3))
* skip rehashing build_index sources already verified against MANIFEST ([#55](https://github.com/Mokto/opengeocoding/issues/55)) ([48e4118](https://github.com/Mokto/opengeocoding/commit/48e4118f46d960f218b584cf3476be28ef54779b))

## [0.4.0](https://github.com/Mokto/opengeocoding/compare/v0.3.0...v0.4.0) (2026-10-14)


### Features

* add SuggestAddress RPC for as-you-type address completion ([#43](https://github.com/Mokto/opengeocoding/issues/43)) ([8b01173](https://github.com/Mokto/opengeocoding/commit/8b01173e9b4a38718f1f9274ce3163e19368cff4))


### Bug Fixes

* stop confident wrong answers and recover results lost to junk tokens, format postcodes, answer territories under their sovereign ([#47](https://github.com/Mokto/opengeocoding/issues/47)) ([4957ceb](https://github.com/Mokto/opengeocoding/commit/4957cebcd1c9405d158d02033d1e5489d7f6e5ed))
* verify a sample of the built index's places, not a single one ([#41](https://github.com/Mokto/opengeocoding/issues/41)) ([a0a640f](https://github.com/Mokto/opengeocoding/commit/a0a640fc67d1ba329484fc8805565ecb91615a04))

## [0.3.0](https://github.com/Mokto/opengeocoding/compare/v0.2.1...v0.3.0) (2026-10-14)


### Features

* let the build Job reuse an existing scratch claim ([#38](https://github.com/Mokto/opengeocoding/issues/38)) ([f7bdc29](https://github.com/Mokto/opengeocoding/commit/f7bdc294c5c83bd8b62c9d6f55300512826585d2))
* log download progress per file ([#39](https://github.com/Mokto/opengeocoding/issues/39)) ([4575d2c](https://github.com/Mokto/opengeocoding/commit/4575d2c2690769e2bbaa0bd7be64339f22e25fa6))
* name ISO 3166-2 subdivisions Overture has no region code for ([#36](https://github.com/Mokto/opengeocoding/issues/36)) ([986b832](https://github.com/Mokto/opengeocoding/commit/986b832f4ecde3215bcaddf11c1b1c62d0150f02))
* replace GetRegionMap and RefreshRegionMap with typed ListRegions ([#34](https://github.com/Mokto/opengeocoding/issues/34)) ([99fb364](https://github.com/Mokto/opengeocoding/commit/99fb36476fb785eb64907d7beb71fd3a43ac7fa4))

## [0.2.1](https://github.com/Mokto/opengeocoding/compare/v0.2.0...v0.2.1) (2026-10-14)


### Bug Fixes

* keep AWS_ALLOW_HTTP when configuring the S3 store ([#32](https://github.com/Mokto/opengeocoding/issues/32)) ([706612d](https://github.com/Mokto/opengeocoding/commit/706612ddef98d8ae0ba481308f4282569a8eacce))

## [0.2.0](https://github.com/Mokto/opengeocoding/compare/v0.1.0...v0.2.0) (2026-10-14)


### Features

* add OSM, OpenAddresses and GeoNames postcodes to the CI index fixture ([#28](https://github.com/Mokto/opengeocoding/issues/28)) ([ca04ac9](https://github.com/Mokto/opengeocoding/commit/ca04ac92f3a052eb4944f3a45093aaa9e07ce92f))


### Performance Improvements

* merge index segments, filter kind in the collector, cache parses, faster startup and image builds ([#30](https://github.com/Mokto/opengeocoding/issues/30)) ([85c56d7](https://github.com/Mokto/opengeocoding/commit/85c56d7a54ecba973d869894c076de07ef1a06de))
