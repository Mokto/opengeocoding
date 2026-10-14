# Changelog

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
