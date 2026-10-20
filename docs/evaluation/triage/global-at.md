# Austria in the global OSM address panel

Triage for [#222](https://github.com/Mokto/opengeocoding/issues/222), which reports Austrian autocomplete at 0/100. All counts below come from one local index (builder `0ae14174be4dd085`), one repeat, with each binary run against the same index.

## 0/100 was fixed by #215

On #219's head (`30ed1e5`) SuggestAddress returned nothing for all 100 Austrian queries, and for all 100 Australian ones. Every panel query ends in the country's English name. That code rejected a last part naming a country when a region's name contained that name as a later word: "Lower Austria" and "Upper Austria" disqualified "Austria", and "South Australia" disqualified "Australia", so the street search stopped. #215 merged after #219's head was measured. It now rejects such a country name only when a region has exactly that name ("Georgia"). On main `50036a3` Austria passes 92/100.

## Towns that are parts of the houses' municipality

Two training groups (4 queries) name a town that the index has as a part of the municipality its houses are filed under. Forward search found one of them:

- "Bahnstraße 20, Wagram am Wagram, 3483": Wagram am Wagram is a cadastral community of Grafenwörth. The house is indexed under Grafenwörth, 17 m from the OSM node.
- "Gerberweg 6, Rohrbach in Oberösterreich, 4150": the town merged into Rohrbach-Berg in 2015. The house is indexed under Rohrbach-Berg, 15 m from the node.

Suggestions scoped to the named place and the places inside it, so they could not find these houses. With a postcode, the town was also not looked up alone. Now, when a postcode is typed and the named town has no such house, SuggestAddress looks in the settlements that contain the town, still within the postcode. A town the index does not know still finds nothing ("15 Township Lane, 60001 Elsewhere"). Austria then passes 96/100. Across the whole gold suite, 37 suggestions newly pass (France, Belgium, Spain, Austria, Finland and Brazil) and none fail or get worse. Search, ResolveRegion and ReverseGeocode are unchanged.

## Remaining failures (4 queries, 2 groups)

| Query | Class | Evidence |
| --- | --- | --- |
| Am Sonnberg 23, Imst, 6460 | Index coverage | The index has Am Sonnberg without house numbers. Search answers the street (`too_coarse`). |
| Gratlift 3, Sölden, 6450 (held out) | Point disagreement | House 3 is found 1.4 km from the OSM node (`near_miss`). This group is held out, so it was not used for tuning. |
