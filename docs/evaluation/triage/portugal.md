# Portugal address search and autocomplete

Triage of the Portugal rows in [#222](https://github.com/Mokto/opengeocoding/issues/222), on the 100 `global_osm_pt` cases (50 groups, 11 held out). Inspection (`evaluate inspect`) and probes used only the 39 tuning groups; held-out groups were scored, not inspected.

## Cause

Every Portuguese house in the index comes from one source, Overture's `pt/countrywide` addresses (5.6 million rows), which name streets as the national register (CTT) writes them: the leading street type abbreviated (`R`, `AV`, `TV`, `PCT`, `BR`, `CAM`, `ESTR`, `LG`, `URB`, `SIT`, `PC`, …), most titles abbreviated (`TEN`, `COMEND`, `CMDT`, `FR`, `COR`, `D` for Dom/Dona, `NSA` for Nossa, `S` for São, Santa and Santo), and usually without the particle after the type (`R IGREJA` 16,143 rows, `R DA IGREJA` 3,547; inner particles mostly stay, `R 25 DE ABRIL` 27,101 against `R 25 ABRIL` 2,611). The map's streets keep the full name (`Rua dos Farias`). Every query word must be in a street's name, so `12 Rua dos Farias` found the map's street, which has no houses, and answered at street level. The existing alternative spellings ran only when no street matched at all.

A second, parser defect: libpostal reads a number written before `Avenida` as part of the road (`6 avenida da noruega`, no house number); `Rua`, `Av.`, `Travessa` and the other types split correctly.

## Fix

- `matching::road_expansions` gives a Portuguese road its register form: the type at the start (after a typed house number) and the titles the register abbreviates in the large majority of its rows, Santa/Santo as `S`, and the particle after the type dropped. `Marquês` (also a surname), `Monsenhor`, `Major` and `Padre` stay whole, as most register rows write them.
- `Index::streets` tries those spellings when the stated road has no such house, not only when no street matched. The existing guards hold: a stated town or a known postcode, a house that exists, a postcode not contradicted, and no answer when two spellings give different streets.
- SuggestAddress tries the register form when a typed Portuguese address with a number finds no house. Only for Portugal: South Africa's `straat`/`street` expansion turns back on itself, which made a general version recurse until it timed out.
- With Portugal stated, a number before `avenida` in the road becomes the house number. Not elsewhere: `6 Avenida` is a whole street name in Guatemala.

The builder and index are unchanged.

## Results

Local full index `74a1eec034f5` (builder `9d4006a4eec15abb`), same evaluator, models and reference data on both sides, base `7a02601`:

| `global_osm_pt` | Base | Candidate |
|---|---:|---:|
| SearchAddress | 22/100 | 46/100 |
| tuning groups | 9/39 | 19/39 |
| held-out groups | 2/11 | 4/11 |
| SuggestAddress (full) | 22/100 | 46/100 |
| ReverseGeocode country | 100/100 | 100/100 |

The full 21,282-case gold diff has no newly failing or worse cases. All 48 newly passing checks are Portuguese; the one changed unscored address case, `4 Rua da Imprensa à Estrela, Portugal`, now resolves to house 4 on the register's `R IMPRENSA A ESTRELA S BENTO` (1200-873) about 100 m from the street point it gave before. Local timings were taken on a shared host under heavy load and are not a speed measurement.

## Remaining tuning failures (20 groups)

- **Number absent from the index (18):** no street of the name within 150 m holds the number: Rua da República 15 (Mirandela) and 18 (5200-233), Rua do Corredouro 39, Praceta Frei Rodrigo Egídio 8, Av. Gonçalo Rodrigues Caldeira 19, Rua São Nicolau 18, Rua das Amendoeiras 107, Avenida dos Combatente do Ultramar 1, Avenida Nacional 100, Rua Teófilo Carvalho dos Santos 28, Rua Senhora da Abadia 473, Rua da Ceifeira 3, Largo Doutor Augusto de Oliveira Coimbra 8, Rua Marquês de Pombal 51, Rua da Valinha 90, Rua Comendador Joaquim Mendes Almeida 15, Rua do Queiró 9, and Rua Principal 79 (Santalha), whose nearby street has no houses, so a same-named street 2.4 km away answers. The register street is present in most of these with other numbers; this is source coverage, not a name mismatch.
- **Names this fix does not bridge (2):** `Avenida Virgílio Ferreira 2` is indexed 3 m away as `Avenida Vergílio Ferreira` (a spelling difference between sources; the misspelling repair runs only when no street matches), and `Rua da GNR 23` as `R DA GUARDA NACIONAL REPUBLICANA` 84 m away (an acronym). Neither is generalized here.

## Native address forms

The panel writes every query in two synthetic orders ending in `, Portugal`. Portuguese addresses are usually written otherwise: `Rua da República, 15, 5370-347 Mirandela`, `Rua José Relvas nº 13, …`, `R. dos Farias 12, …`, often without the country. Seven such forms of each passing panel group (133 tuning queries from 19 groups; the 28 queries of the 4 passing held-out groups were only scored), answered by the same index:

| | SearchAddress before | after | SuggestAddress before | after |
|---|---:|---:|---:|---:|
| tuning | 75/133 | 131/133 | 12/133 | 131/133 |
| held-out | 16/28 | 28/28 | 4/28 | 28/28 |

- Without a stated country, a postcode written `NNNN-NNN` (no other country writes them so) now lets search and SuggestAddress read the road as the register does. A town alone, or a postcode another country could share (`50667`), implies nothing: the resolver retries a query without its country, and a country-scoped spelling found that way would override a stated country.
- SuggestAddress reads a house number written as its own part (`Rua X, 12, …`, at most four digits and a letter, so not a postcode), skips `nº`, `n.º` and `n°`, and splits a part with no commas before a known postcode that follows the number (`Rua dos Farias 12 2955-243 Pinhal Novo`). A postcode with no town after it now scopes the house too.
- Not handled: `B.º` for Bairro (2 queries).

On the full gold suite, SearchAddress is unchanged (its queries state the country) and SuggestAddress gains 34 checks with none lost: 31 Dutch queries written without commas (`Chopinstraat 51 5481LP Schijndel Netherlands`), two Spanish and one Italian with the number as its own part (`Via Bernardo Clesio, 5, 38122 Trento, Italy`).

Still open without a country: a query whose number the index lacks can get no answer at all when the postcode's place and the stated town differ in name (`Estrada de Mem Martins 9999, 2725-377 Mem Martins`; 3 of 39 such probes), because accepting the postcode for that town requires a stated country. That is a resolver rule for every country, left for a separate change.

## Source coverage

The 18 absent numbers are coverage, not handling. Portugal's addresses come only from the register; Portugal is among the `osmLandmarks` extracts, read for landmarks only, so OpenStreetMap's Portuguese addresses never reach the index. Adding `portugal` to the `osm` sources would fill some of these gaps, but this panel is drawn from the same OpenStreetMap address nodes, so its Portuguese score would then largely measure the source agreeing with itself: that would be a data change to judge on other evidence, separately from this code.
