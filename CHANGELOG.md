# Changelog

## [1.0.0](https://github.com/Mokto/opengeocoding/compare/v0.20.3...v1.0.0) (2026-10-28)


### Features

* read Korean shop-building addresses from the Small Enterprise and Market Service register ([#350](https://github.com/Mokto/opengeocoding/issues/350)) ([476a4f3](https://github.com/Mokto/opengeocoding/commit/476a4f3786dd8728ff6915a8b739a336eb915daf))


### Bug Fixes

* answer people-profile place strings in the country they state ([#369](https://github.com/Mokto/opengeocoding/issues/369)) ([2d6c288](https://github.com/Mokto/opengeocoding/commit/2d6c2880a12c500e401d2bd707801415d99b3aef))
* **deps:** update parquet's zstd for RUSTSEC-2026-0391 and document the unreachable connectrpc path ([#362](https://github.com/Mokto/opengeocoding/issues/362)) ([4d0febf](https://github.com/Mokto/opengeocoding/commit/4d0febfa443089e8705c90bcc78495feed13d8fc))
* find Estonian houses on streets written with tn, mnt or pst ([#366](https://github.com/Mokto/opengeocoding/issues/366)) ([29a42b1](https://github.com/Mokto/opengeocoding/commit/29a42b18fe3bad0b45a54b627d114dde5e1307ac)), closes [#354](https://github.com/Mokto/opengeocoding/issues/354)
* keep Korean land-lot aliases out of reverse lookups ([#370](https://github.com/Mokto/opengeocoding/issues/370)) ([925ce27](https://github.com/Mokto/opengeocoding/commit/925ce2796bb7ffd3cbaeb24f3cb357b9eda16be0))
* mirror an OpenAddresses layer the catalogue lists twice only once ([#367](https://github.com/Mokto/opengeocoding/issues/367)) ([2e04c82](https://github.com/Mokto/opengeocoding/commit/2e04c82287fa4548c50841499114ec3329176472))
* stop sealing corrupt OSM extracts from resumed downloads ([#373](https://github.com/Mokto/opengeocoding/issues/373)) ([33dd67c](https://github.com/Mokto/opengeocoding/commit/33dd67cace70b77330def775987dcf91b90a03c0))


### Performance Improvements

* halve the index builder's peak memory ([#371](https://github.com/Mokto/opengeocoding/issues/371)) ([6053415](https://github.com/Mokto/opengeocoding/commit/6053415134734fea615efd6533dfeefa87e5b258))
* lower the index builder's peak memory ([#368](https://github.com/Mokto/opengeocoding/issues/368)) ([958a928](https://github.com/Mokto/opengeocoding/commit/958a928aba85290527329da4936c4780fbe8b598))


### Documentation

* drop the experimental label for 1.0.0 and state what semver covers ([#352](https://github.com/Mokto/opengeocoding/issues/352)) ([e9e8ed6](https://github.com/Mokto/opengeocoding/commit/e9e8ed6dc73dd096bc28f801152c580c19f300ce))

## [0.20.3](https://github.com/Mokto/opengeocoding/compare/v0.20.2...v0.20.3) (2026-10-25)


### Bug Fixes

* index Overture release 2026-10-21.0 ([#334](https://github.com/Mokto/opengeocoding/issues/334)) ([229ee49](https://github.com/Mokto/opengeocoding/commit/229ee493f6ae439a7801e7229f55e7898179c9e5))

## [0.20.2](https://github.com/Mokto/opengeocoding/compare/v0.20.1...v0.20.2) (2026-10-25)


### Bug Fixes

* find Korean houses whose 리 the data puts outside the stated 읍 or 면 ([#337](https://github.com/Mokto/opengeocoding/issues/337)) ([d09301b](https://github.com/Mokto/opengeocoding/commit/d09301b89ea7f354b0aeeff0fa2ba84e4892964e))
* read Kosovo addresses, suggest Palestinian houses under Palestine, and index Bermuda, the Falklands, Jersey, Guernsey and the Isle of Man from OSM ([#342](https://github.com/Mokto/opengeocoding/issues/342)) ([9bd987a](https://github.com/Mokto/opengeocoding/commit/9bd987a537f84bb1ecdc811de5972be9461974bc))
* suggest Bogotá houses by their plate, as the register keys them ([#336](https://github.com/Mokto/opengeocoding/issues/336)) ([0f62cf2](https://github.com/Mokto/opengeocoding/commit/0f62cf2d017b37563dc89be06f82279e99e366d4))
* suggest houses of whole addresses written country-first, without commas or a letter off, with a Belgian box or a Brazilian state ([#344](https://github.com/Mokto/opengeocoding/issues/344)) ([855d212](https://github.com/Mokto/opengeocoding/commit/855d212371a7dab8b7014f833964538754050341)), closes [#333](https://github.com/Mokto/opengeocoding/issues/333)
* update tokio for I/O safety advisory ([#347](https://github.com/Mokto/opengeocoding/issues/347)) ([5e2cc5d](https://github.com/Mokto/opengeocoding/commit/5e2cc5d2820714079bf670b82ff76fe7f8ecf389))


### Performance Improvements

* try a Colombian plate once the street lacks the written house ([#339](https://github.com/Mokto/opengeocoding/issues/339)) ([a491c60](https://github.com/Mokto/opengeocoding/commit/a491c6087cc48e65fbfdbf1fb2727e6e76e3286b))

## [0.20.1](https://github.com/Mokto/opengeocoding/compare/v0.20.0...v0.20.1) (2026-10-22)


### Bug Fixes

* do not suggest another village's house when a place is left out of a full address ([#331](https://github.com/Mokto/opengeocoding/issues/331)) ([08b7372](https://github.com/Mokto/opengeocoding/commit/08b7372786ce3e6efdeb7eed0301c5ef2412f500))
* read Angola, Dominica and Congo as themselves, drop calling codes written as postcodes, and suggest houses past an extra place ([#329](https://github.com/Mokto/opengeocoding/issues/329)) ([7351483](https://github.com/Mokto/opengeocoding/commit/7351483ceb94e9f8cf800345e592b5a3d7a2aa77))
* spanish house extensions and address-area queries in search and suggest ([#322](https://github.com/Mokto/opengeocoding/issues/322)) ([c6c8810](https://github.com/Mokto/opengeocoding/commit/c6c8810a5da0ead21da4636a25e455508f40ace1))
* suggest Italian addresses written with a province sigla or a frazione ([#326](https://github.com/Mokto/opengeocoding/issues/326)) ([008aad0](https://github.com/Mokto/opengeocoding/commit/008aad086cc3b03bb30931e8c93db383fd6df5f8))

## [0.20.0](https://github.com/Mokto/opengeocoding/compare/v0.19.0...v0.20.0) (2026-10-22)


### Features

* report each source mirror's progress in files ([#309](https://github.com/Mokto/opengeocoding/issues/309)) ([f575847](https://github.com/Mokto/opengeocoding/commit/f5758479ee90c4ef7b6660302b4b6c0500bb6175))


### Bug Fixes

* **deps:** update non-major dependencies ([#316](https://github.com/Mokto/opengeocoding/issues/316)) ([04f9f6d](https://github.com/Mokto/opengeocoding/commit/04f9f6d63827f8721e8ecffdbdacbf82fc765044))
* find Berlin's lettered houses and numbered streets ([#307](https://github.com/Mokto/opengeocoding/issues/307)) ([83044da](https://github.com/Mokto/opengeocoding/commit/83044dacf3f3cc5ef1943757fcf2d510e66ff001))
* find Italian civic numbers with an exponent and addresses that name a frazione ([#321](https://github.com/Mokto/opengeocoding/issues/321)) ([188b351](https://github.com/Mokto/opengeocoding/commit/188b3516d1013252e8170edc4426e74c9b8bb718))
* read the country libpostal takes for a road after a postcode (Albania, North Macedonia) ([#317](https://github.com/Mokto/opengeocoding/issues/317)) ([8384799](https://github.com/Mokto/opengeocoding/commit/83847997b4f540c1bc179d4a7a29479c8e4e7321))
* suggest Egyptian addresses written with hamza variants and district prefixes ([#315](https://github.com/Mokto/opengeocoding/issues/315)) ([e8d9814](https://github.com/Mokto/opengeocoding/commit/e8d9814de47bf230d38e8d5a6ecc62c65d895990))
* suggest US addresses with units, state codes and ZIP+4, and UK postcodes with their district ([#320](https://github.com/Mokto/opengeocoding/issues/320)) ([5e1ebdf](https://github.com/Mokto/opengeocoding/commit/5e1ebdf4d706d792f88d4cdf1de1fb2d75ea7a85))
* time gold speed cases only after the suggestion warm-up ([#319](https://github.com/Mokto/opengeocoding/issues/319)) ([4a9b0bb](https://github.com/Mokto/opengeocoding/commit/4a9b0bb61998207270d68ee9818904bc9fd4471e))

## [0.19.0](https://github.com/Mokto/opengeocoding/compare/v0.18.1...v0.19.0) (2026-10-21)


### Features

* add a Berlin house-coordinate gold corpus for Germany ([#304](https://github.com/Mokto/opengeocoding/issues/304)) ([2810c15](https://github.com/Mokto/opengeocoding/commit/2810c153b12d04a17af8f26f216cbf88b772eb28))
* split every country's gold pass rates by evidence and keep a history in the nightly issue ([#297](https://github.com/Mokto/opengeocoding/issues/297)) ([0eca74f](https://github.com/Mokto/opengeocoding/commit/0eca74fb0ef58785b247e1d25bf9528cfacb8686))


### Bug Fixes

* fetch an OSM extract added under a sealed snapshot on its own ([#308](https://github.com/Mokto/opengeocoding/issues/308)) ([577440b](https://github.com/Mokto/opengeocoding/commit/577440b0677296dc51a01149c23b03ef0ea46323))
* index OSM addresses for Kosovo, Serbia and the Antilles ([#301](https://github.com/Mokto/opengeocoding/issues/301)) ([6dcf747](https://github.com/Mokto/opengeocoding/commit/6dcf747b0350c979b1f76161ca21097f80ae4532))
* index Uruguay's OSM addresses ([#303](https://github.com/Mokto/opengeocoding/issues/303)) ([a6229da](https://github.com/Mokto/opengeocoding/commit/a6229dae3be1a1da4ff2795466ccafaaaad9dab9))
* suggest a Luxembourg house when the postcode is written after the town ([#305](https://github.com/Mokto/opengeocoding/issues/305)) ([d397ffa](https://github.com/Mokto/opengeocoding/commit/d397ffae393af6b65608d78fccd5ad587720300b))

## [0.18.1](https://github.com/Mokto/opengeocoding/compare/v0.18.0...v0.18.1) (2026-10-21)


### Bug Fixes

* delete an earlier build's sources from a kept scratch volume ([#259](https://github.com/Mokto/opengeocoding/issues/259)) ([f3b90f1](https://github.com/Mokto/opengeocoding/commit/f3b90f1a32afa941f7e8bc12af72da370eb1ac90))
* **deps:** update rustls, rustls-webpki and webpki roots for RUSTSEC-2026-0346 to 0349 and 0381 to 0383 ([0b331b3](https://github.com/Mokto/opengeocoding/commit/0b331b35dbeed0edd4d8a9ab1c113bb6541c66aa))
* find Arabic houses written with the number before the street ([#262](https://github.com/Mokto/opengeocoding/issues/262)) ([d205d6d](https://github.com/Mokto/opengeocoding/commit/d205d6d7031ef50b559e106c0211f4b840944839))
* find Czech houses by orientation number alone and by č.p. or č.ev. mark ([#260](https://github.com/Mokto/opengeocoding/issues/260)) ([454ed5b](https://github.com/Mokto/opengeocoding/commit/454ed5bc1ad90e547eadf180afcc3560411fbacc))
* find Irish provinces by name and keep Wallis and Futuna district codes ([#167](https://github.com/Mokto/opengeocoding/issues/167)) ([#251](https://github.com/Mokto/opengeocoding/issues/251)) ([8307ab0](https://github.com/Mokto/opengeocoding/commit/8307ab09eb58e035684ad034470402c56fb359b5))
* index Brazil's OSM addresses ([#284](https://github.com/Mokto/opengeocoding/issues/284)) ([15c66ce](https://github.com/Mokto/opengeocoding/commit/15c66ce6b4406c2d0abd221bcd899fec94cf1ef4))


### Performance Improvements

* trust hard-linked store files by size instead of rehashing them ([#255](https://github.com/Mokto/opengeocoding/issues/255)) ([d17f3b5](https://github.com/Mokto/opengeocoding/commit/d17f3b568e0ca9f2230c15688247dc80e09c93ab))

## [0.18.0](https://github.com/Mokto/opengeocoding/compare/v0.17.1...v0.18.0) (2026-10-21)


### Features

* find houses the parse misses in Indian and other civic-number addresses ([#244](https://github.com/Mokto/opengeocoding/issues/244)) ([dc63545](https://github.com/Mokto/opengeocoding/commit/dc635458a347bdfea4ae0835c3098503939b0e80))


### Bug Fixes

* find Bogotá houses on a road whose name another locality shares ([#246](https://github.com/Mokto/opengeocoding/issues/246)) ([0b93cbd](https://github.com/Mokto/opengeocoding/commit/0b93cbd7d8c471fcdd9906065c36f84f73ed693a))
* find Bogotá houses written with plate numbers and avenue road types ([#236](https://github.com/Mokto/opengeocoding/issues/236)) ([8298239](https://github.com/Mokto/opengeocoding/commit/82982395fed3f89227f3ffa9573dd8d55401f160))
* find New Zealand houses by their town, a number after the road and an unheld postcode ([#247](https://github.com/Mokto/opengeocoding/issues/247)) ([e8a18f9](https://github.com/Mokto/opengeocoding/commit/e8a18f9e31e6cc8d4eace56fbf74a1fb3770e3c4))
* find Taiwanese houses written with Taiwan Province or a number after the road ([#250](https://github.com/Mokto/opengeocoding/issues/250)) ([519c5c9](https://github.com/Mokto/opengeocoding/commit/519c5c9637f8ca732757ba78a5da0f3898dc81b4))
* index Finland's OpenStreetMap addresses ([#249](https://github.com/Mokto/opengeocoding/issues/249)) ([7a6beca](https://github.com/Mokto/opengeocoding/commit/7a6beca61e4d3359e498bbbbc644f7988ec7cb78))
* index OSM addresses for Chile, Colombia, Mexico, Portugal and Taiwan ([#233](https://github.com/Mokto/opengeocoding/issues/233)) ([87f788e](https://github.com/Mokto/opengeocoding/commit/87f788e64da1119715a1e9563c3b0b78be86f42e))
* let a postcode and town state the country a query leaves out ([#242](https://github.com/Mokto/opengeocoding/issues/242)) ([2121761](https://github.com/Mokto/opengeocoding/commit/21217610620963f732bf2b51bb3bd55fbd099a45))
* spanish address search: place cadastre points, match registry street spellings, tell villages apart ([#226](https://github.com/Mokto/opengeocoding/issues/226)) ([403b6ef](https://github.com/Mokto/opengeocoding/commit/403b6ef18e8f511ccd9683a295e10aa6bd225f8e))
* suggest houses for a town that is part of their municipality, within the typed postcode ([#243](https://github.com/Mokto/opengeocoding/issues/243)) ([f654284](https://github.com/Mokto/opengeocoding/commit/f65428461aa20208ac1a6d4f77a505f052d3209c))
* suggest houses whose number keeps a space before its letter ([#248](https://github.com/Mokto/opengeocoding/issues/248)) ([bf4fd3a](https://github.com/Mokto/opengeocoding/commit/bf4fd3aa2cca85b10b04a356f2f6cb4b396e84de))
* take an extract moved between the OSM lists from the other sealed mirror ([#240](https://github.com/Mokto/opengeocoding/issues/240)) ([fafe73b](https://github.com/Mokto/opengeocoding/commit/fafe73b0349b675d9b6e586bdde23a1685344d39))


### Performance Improvements

* **evaluate:** load the direct index and probe grpcurl while the service starts ([#235](https://github.com/Mokto/opengeocoding/issues/235)) ([50036a3](https://github.com/Mokto/opengeocoding/commit/50036a3f1880f401f3d3fbf2ce91616c63450aa7))
* read landmark extracts beside the build's other passes ([#241](https://github.com/Mokto/opengeocoding/issues/241)) ([70cb37b](https://github.com/Mokto/opengeocoding/commit/70cb37b141b1c6935cd354c52679591f46360844))

## [0.17.1](https://github.com/Mokto/opengeocoding/compare/v0.17.0...v0.17.1) (2026-10-20)


### Bug Fixes

* answer Canadian suffixed civic numbers and French street directions ([#230](https://github.com/Mokto/opengeocoding/issues/230)) ([a0da3e0](https://github.com/Mokto/opengeocoding/commit/a0da3e07327673e05f8d55a1604fc6a72c1d2ab1))
* index Japan's OpenStreetMap addresses ([#228](https://github.com/Mokto/opengeocoding/issues/228)) ([8d50bdd](https://github.com/Mokto/opengeocoding/commit/8d50bdd82c78052a03b6291097d57320c3b70ae2))
* read a Japanese town or road and its number in either order ([#229](https://github.com/Mokto/opengeocoding/issues/229)) ([8efeafa](https://github.com/Mokto/opengeocoding/commit/8efeafa1ac5226416148aff821ad2db84b887947))
* resolve and suggest Portuguese addresses as they are usually written ([#231](https://github.com/Mokto/opengeocoding/issues/231)) ([43cc3e8](https://github.com/Mokto/opengeocoding/commit/43cc3e8914ef736e5bed48c0253fbf176ff64548))
* resolve AU streets written before their number to the right street ([#223](https://github.com/Mokto/opengeocoding/issues/223)) ([00818d1](https://github.com/Mokto/opengeocoding/commit/00818d198f9627e6e0fd55da0bedbadde1cae0dd))

## [0.17.0](https://github.com/Mokto/opengeocoding/compare/v0.16.0...v0.17.0) (2026-10-20)


### Features

* add a 48-market OSM address source-consistency benchmark to gold ([#219](https://github.com/Mokto/opengeocoding/issues/219)) ([7a02601](https://github.com/Mokto/opengeocoding/commit/7a02601cfd50b6cc3034febcfada7075c4daa6ae))
* import historical memberships from official datasets ([#218](https://github.com/Mokto/opengeocoding/issues/218)) ([5db596c](https://github.com/Mokto/opengeocoding/commit/5db596c5982ed35eb5056281c782c76f4321fee9)), closes [#216](https://github.com/Mokto/opengeocoding/issues/216)
* resolve and suggest Australian unit, state-code and suburb addresses ([#215](https://github.com/Mokto/opengeocoding/issues/215)) ([98595d0](https://github.com/Mokto/opengeocoding/commit/98595d0f8b7fe68d98b1bdc222cb0ea20edd31e6))
* show country names and pass-rate percentages in the gold diff country table ([#221](https://github.com/Mokto/opengeocoding/issues/221)) ([2d5b95d](https://github.com/Mokto/opengeocoding/commit/2d5b95d4141e02616f58bec4dcbb2aabd8f17a03))


### Bug Fixes

* answer Polish village, ul.-prefixed and ulica-named street addresses with the exact house ([#211](https://github.com/Mokto/opengeocoding/issues/211)) ([8240db2](https://github.com/Mokto/opengeocoding/commit/8240db2183b7626d6feabc338c52c257ad53183b))
* find Portuguese houses under the national register's abbreviated street names ([#224](https://github.com/Mokto/opengeocoding/issues/224)) ([94e6674](https://github.com/Mokto/opengeocoding/commit/94e6674c7128bcd055e37c5a21a908736488edf2))
* suggest houses on date-named roads and keep the best street part first ([#213](https://github.com/Mokto/opengeocoding/issues/213)) ([ef1d055](https://github.com/Mokto/opengeocoding/commit/ef1d0553f0b41e057b40cc294a79e9e2f42a6a1e))


### Performance Improvements

* time a 2,000-case sample and check the rest of the gold suite 16 at a time ([#217](https://github.com/Mokto/opengeocoding/issues/217)) ([62345f3](https://github.com/Mokto/opengeocoding/commit/62345f3c3ca04963b4888c060111591a7ffe8e98))

## [0.16.0](https://github.com/Mokto/opengeocoding/compare/v0.15.0...v0.16.0) (2026-10-20)


### Features

* add Canadian and Polish registry gold corpora ([#207](https://github.com/Mokto/opengeocoding/issues/207)) ([6ef3b5a](https://github.com/Mokto/opengeocoding/commit/6ef3b5a30a90791474900be48cdad23b2e00427a))
* recover places through reviewed historical memberships ([#198](https://github.com/Mokto/opengeocoding/issues/198)) ([3671e1c](https://github.com/Mokto/opengeocoding/commit/3671e1c8cb17b08bf14168c5077aa9b230038395))
* report passing checks per expected country in the gold diff ([#212](https://github.com/Mokto/opengeocoding/issues/212)) ([8cbb358](https://github.com/Mokto/opengeocoding/commit/8cbb3589ac4d5caae9296e61b16741e2ef7b8312))
* suggest native Taipei buildings with unverified floors ([#205](https://github.com/Mokto/opengeocoding/issues/205)) ([dc7dca3](https://github.com/Mokto/opengeocoding/commit/dc7dca30ba16e1bce19a92337955296c11892a9a))


### Bug Fixes

* complete addresses with joined city and region qualifiers ([#214](https://github.com/Mokto/opengeocoding/issues/214)) ([f9b47dd](https://github.com/Mokto/opengeocoding/commit/f9b47dd0ff3855b2732e03e9600d4bd22a77d66e))
* recover Canadian unit-address buildings ([#209](https://github.com/Mokto/opengeocoding/issues/209)) ([02d0d3f](https://github.com/Mokto/opengeocoding/commit/02d0d3f2d1df6fe2178d95fdc6d255fa283ad3a4))
* resolve and suggest remaining Danish unit and supplementary-place addresses ([#210](https://github.com/Mokto/opengeocoding/issues/210)) ([a98637c](https://github.com/Mokto/opengeocoding/commit/a98637c312d69e37c1501bb19313b735eafadb25))
* resolve Danish floor and door addresses to their building's house ([#206](https://github.com/Mokto/opengeocoding/issues/206)) ([6116516](https://github.com/Mokto/opengeocoding/commit/611651668a0ad28fb381e01234ab450234ea4895))
* retain New Zealand unit addresses from source fields ([#204](https://github.com/Mokto/opengeocoding/issues/204)) ([9b32941](https://github.com/Mokto/opengeocoding/commit/9b3294106c6ac35c39b80714792a528fc8a870fc))

## [0.15.0](https://github.com/Mokto/opengeocoding/compare/v0.14.0...v0.15.0) (2026-10-19)


### Features

* add Australian, Danish and Spanish registry gold corpora ([#202](https://github.com/Mokto/opengeocoding/issues/202)) ([7f3e72b](https://github.com/Mokto/opengeocoding/commit/7f3e72b555d6bd1b8866ab5e155a85443eb197eb))
* add Belgian, New Zealand, Taipei and Bogota registry gold corpora ([#190](https://github.com/Mokto/opengeocoding/issues/190)) ([96dc059](https://github.com/Mokto/opengeocoding/commit/96dc059dd631c4a5d93552d0bd88e29485389caa))


### Bug Fixes

* native Taipei addresses abstaining or returning Taiwan's centroid ([#201](https://github.com/Mokto/opengeocoding/issues/201)) ([8055894](https://github.com/Mokto/opengeocoding/commit/8055894616655fdbff0149de6c97dbcad7792877))
* repair New Zealand suburb and town comma addresses ([#200](https://github.com/Mokto/opengeocoding/issues/200)) ([5e96fbe](https://github.com/Mokto/opengeocoding/commit/5e96fbe74c60bd5bb2d290559047a6b8635502fa))

## [0.14.0](https://github.com/Mokto/opengeocoding/compare/v0.13.0...v0.14.0) (2026-10-19)


### Features

* add exact suggestion retrieval ([#191](https://github.com/Mokto/opengeocoding/issues/191)) ([097e167](https://github.com/Mokto/opengeocoding/commit/097e167521c3bbd6f1ce4aa777b95f2bc088de1a))

## [0.13.0](https://github.com/Mokto/opengeocoding/compare/v0.12.1...v0.13.0) (2026-10-19)


### Features

* resolve "South Denmark" as the Danish region DK-83 ([#187](https://github.com/Mokto/opengeocoding/issues/187)) ([69dcf9a](https://github.com/Mokto/opengeocoding/commit/69dcf9a82fdc7d9ce0a7f8607b39c8d1ce8558b5))


### Bug Fixes

* recover remaining source-backed postal-town houses ([#186](https://github.com/Mokto/opengeocoding/issues/186)) ([589c018](https://github.com/Mokto/opengeocoding/commit/589c018964ac3183efe3bf15c6a8b1384e2688ef))
* resolve native Korean road and land-lot addresses within their administrative hierarchy ([#184](https://github.com/Mokto/opengeocoding/issues/184)) ([a165cab](https://github.com/Mokto/opengeocoding/commit/a165cabff55336cddf017f12d3cad3e4303081b7))
* resolve subdivision-coded place names and correct Zallaq's region ([#189](https://github.com/Mokto/opengeocoding/issues/189)) ([a672736](https://github.com/Mokto/opengeocoding/commit/a6727368e17b7468624fdefacf5346809d05084b))

## [0.12.1](https://github.com/Mokto/opengeocoding/compare/v0.12.0...v0.12.1) (2026-10-19)


### Bug Fixes

* recover dotted str. compounds, damaged strasse suffixes and Straat/Street in road lookup ([#181](https://github.com/Mokto/opengeocoding/issues/181)) ([9794a6d](https://github.com/Mokto/opengeocoding/commit/9794a6d15865376a346e6f7a3a953c3c925e9ce0))
* stop foreign settlements and conflicting parents in import assignment ([#179](https://github.com/Mokto/opengeocoding/issues/179)) ([0491e8f](https://github.com/Mokto/opengeocoding/commit/0491e8f9f7f5a3e13c455801e1f2bd34bde7dbbc))

## [0.12.0](https://github.com/Mokto/opengeocoding/compare/v0.11.1...v0.12.0) (2026-10-18)


### Features

* add offline competitor geocoding comparisons and scorecard ([#144](https://github.com/Mokto/opengeocoding/issues/144)) ([cfc2780](https://github.com/Mokto/opengeocoding/commit/cfc2780e77209297c8455cc8e97d99d067ac98f7))
* add project-local wait_for_gold tool and /gold-loop command for pi ([#183](https://github.com/Mokto/opengeocoding/issues/183)) ([70d1c05](https://github.com/Mokto/opengeocoding/commit/70d1c05fb27186b78d2cc350f84b05b1bd0a1b33))


### Bug Fixes

* correct stale Isa Town region and document unresolved Dairy Lane and Ikorodu Road gold conflicts ([#180](https://github.com/Mokto/opengeocoding/issues/180)) ([dbd46f3](https://github.com/Mokto/opengeocoding/commit/dbd46f3a17fea72dca75b5966887d91dc47aeacd))
* recover indexed place names with scoped qualifiers ([#178](https://github.com/Mokto/opengeocoding/issues/178)) ([aa347e9](https://github.com/Mokto/opengeocoding/commit/aa347e9d34d53d74a4a7f1ff26f29b01bf76625c))

## [0.11.1](https://github.com/Mokto/opengeocoding/compare/v0.11.0...v0.11.1) (2026-10-18)


### Bug Fixes

* keep Danish buildings in autocomplete when a floor suffix is typed ([#171](https://github.com/Mokto/opengeocoding/issues/171)) ([32d1e3f](https://github.com/Mokto/opengeocoding/commit/32d1e3fe299f5782860c7d33a162b2d649958efc))
* keep source postal towns apart from settlement ancestry ([#176](https://github.com/Mokto/opengeocoding/issues/176)) ([7c84e75](https://github.com/Mokto/opengeocoding/commit/7c84e7540f3cf5913ed6bb406ac0e02d369ecd77))
* lower confidence on unexplained roads, recover invalid postcodes, prefer named county ([#172](https://github.com/Mokto/opengeocoding/issues/172)) ([4e2a3d7](https://github.com/Mokto/opengeocoding/commit/4e2a3d782a5732fa8d8d8263fb2855a88419b6da))
* recover country-scoped road abbreviations in SearchAddress ([#175](https://github.com/Mokto/opengeocoding/issues/175)) ([a728336](https://github.com/Mokto/opengeocoding/commit/a728336c1dece1aa8acddce4161c93cb60c8b7c8))
* resolve qualified Norwegian/Dutch road abbreviations and Dutch postcode letters ([#177](https://github.com/Mokto/opengeocoding/issues/177)) ([f7a6727](https://github.com/Mokto/opengeocoding/commit/f7a6727a611ba0d0290291735f736348d6219d20))
* suggest houses when a comma follows the house number ([#174](https://github.com/Mokto/opengeocoding/issues/174)) ([fa25879](https://github.com/Mokto/opengeocoding/commit/fa258795c1dd5667e7ad72483b917852954193d9))

## [0.11.0](https://github.com/Mokto/opengeocoding/compare/v0.10.1...v0.11.0) (2026-10-17)


### Features

* add opt-in SearchAddress diagnostic logging ([#162](https://github.com/Mokto/opengeocoding/issues/162)) ([7e6bd24](https://github.com/Mokto/opengeocoding/commit/7e6bd24df0c1ee25eafa6d53f6aca361cb8ade59))


### Bug Fixes

* corroborate house towns with indexed locality aliases ([#158](https://github.com/Mokto/opengeocoding/issues/158)) ([3960f49](https://github.com/Mokto/opengeocoding/commit/3960f49aae6f2ce1366106edc3000a85287a7827))
* fall back when a house's postcode contradicts the requested one on the same street ([#156](https://github.com/Mokto/opengeocoding/issues/156)) ([40425d8](https://github.com/Mokto/opengeocoding/commit/40425d89902c0f428ae67bfe0515dc39315199bb))

## [0.10.1](https://github.com/Mokto/opengeocoding/compare/v0.10.0...v0.10.1) (2026-10-17)


### Bug Fixes

* break equal-distance house ties by street id, not index layout ([#150](https://github.com/Mokto/opengeocoding/issues/150)) ([50818f6](https://github.com/Mokto/opengeocoding/commit/50818f6c5b61f9b7f6cdcd02b19bb134bc2e7871))
* complete native and numeric places and qualified postcodes ([#154](https://github.com/Mokto/opengeocoding/issues/154)) ([740be6d](https://github.com/Mokto/opengeocoding/commit/740be6d9cfebeecbc28efd25d695ae8ac7c18f61))
* registry address misses in France, Norway and Switzerland ([#151](https://github.com/Mokto/opengeocoding/issues/151)) ([8460735](https://github.com/Mokto/opengeocoding/commit/8460735f5d991c486456b61f11192eb4dfe69238))

## [0.10.0](https://github.com/Mokto/opengeocoding/compare/v0.9.0...v0.10.0) (2026-10-17)


### Features

* add France, Norway and Switzerland registry samples to the gold suite and report added-case rates ([#146](https://github.com/Mokto/opengeocoding/issues/146)) ([1e3d786](https://github.com/Mokto/opengeocoding/commit/1e3d786783222b844872ac414a0ff24a7d45dcc7))
* add opt-in landmark suggestions to SuggestAddress ([#142](https://github.com/Mokto/opengeocoding/issues/142)) ([6667600](https://github.com/Mokto/opengeocoding/commit/66676001ce96eeaa975313f9010e790f5be0b13d))


### Bug Fixes

* corroborate index-recognized postcodes conservatively ([#147](https://github.com/Mokto/opengeocoding/issues/147)) ([4ef106a](https://github.com/Mokto/opengeocoding/commit/4ef106ad821403436adec3f306e6e24f01256262))
* evaluate chart-only pull requests ([#149](https://github.com/Mokto/opengeocoding/issues/149)) ([295e196](https://github.com/Mokto/opengeocoding/commit/295e196a29ad9c35e8b5a9c332b2710a60f066dd))
* match neighbourhood names as the town and build staging from the exact PR head ([#145](https://github.com/Mokto/opengeocoding/issues/145)) ([5ef0e34](https://github.com/Mokto/opengeocoding/commit/5ef0e34db55273e641570b227730b10f04143a41))

## [0.9.0](https://github.com/Mokto/opengeocoding/compare/v0.8.0...v0.9.0) (2026-10-17)


### Features

* answer ResolveRegion from containing administrative polygons ([#138](https://github.com/Mokto/opengeocoding/issues/138)) ([d1857f1](https://github.com/Mokto/opengeocoding/commit/d1857f1e1e661953bc47ca12a02e8bca9e464534))
* answer ReverseGeocode with the nearest street when no house is in range ([#137](https://github.com/Mokto/opengeocoding/issues/137)) ([fbcd914](https://github.com/Mokto/opengeocoding/commit/fbcd91447ebdca1867fb126f10a3facb32f7bb74))
* measure house-number coverage and fill gaps from Overture places ([#143](https://github.com/Mokto/opengeocoding/issues/143)) ([abc5aa2](https://github.com/Mokto/opengeocoding/commit/abc5aa23bb16005d4a64db64a1045c7bc7f47e6e))
* SuggestAddress typo tolerance, focus position bias and later words of names ([#139](https://github.com/Mokto/opengeocoding/issues/139)) ([ce8e15d](https://github.com/Mokto/opengeocoding/commit/ce8e15d66bda68fcc4c61b7cab052963b4e7f94c)), closes [#135](https://github.com/Mokto/opengeocoding/issues/135)


### Bug Fixes

* answer city+postcode queries a foreign postcode site would drop ([#128](https://github.com/Mokto/opengeocoding/issues/128)) ([8eecc74](https://github.com/Mokto/opengeocoding/commit/8eecc74ff8dc3a0370e119ab8df3f1a3ad72f01d))
* list the four Irish provinces as regions and return them in answers ([#129](https://github.com/Mokto/opengeocoding/issues/129)) ([2d0c902](https://github.com/Mokto/opengeocoding/commit/2d0c90273993ccb8310b5e7a32c3260725477d8f))
* set TCP_NODELAY on RPC connections ([#131](https://github.com/Mokto/opengeocoding/issues/131)) ([15c9090](https://github.com/Mokto/opengeocoding/commit/15c90908de70534584fc4f7d299b9d16b722172b))


### Performance Improvements

* hard-link published and pulled directories in a local store ([#140](https://github.com/Mokto/opengeocoding/issues/140)) ([644a34c](https://github.com/Mokto/opengeocoding/commit/644a34c54d39c567ca77edb27acbee497751b86a))

## [0.8.0](https://github.com/Mokto/opengeocoding/compare/v0.7.2...v0.8.0) (2026-10-16)


### ⚠ BREAKING CHANGES

* rename service to opengeocoding.OpenGeocoding ([#125](https://github.com/Mokto/opengeocoding/issues/125))

### Features

* rename service to opengeocoding.OpenGeocoding ([#125](https://github.com/Mokto/opengeocoding/issues/125)) ([7c25390](https://github.com/Mokto/opengeocoding/commit/7c253905f1db0ef01fb37b69ffed137ef1ee8b59))

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
