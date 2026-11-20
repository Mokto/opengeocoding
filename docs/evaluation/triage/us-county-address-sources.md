# US county address points: source review

These OpenAddresses layers remain excluded. A public ArcGIS endpoint or a downloadable batch export does not by itself establish permission to redistribute an index derived from the data.

| OpenAddresses layer | Publisher evidence | What is needed |
| --- | --- | --- |
| `us_ga_fayette-county` | The county's [Address Points item](https://fayettecounty.maps.arcgis.com/home/item.html?id=857591f697524dd1b00cfbef7d00a8e2) is public but has no stated reuse terms. | Obtain the terms for this layer from the county. |
| `us_pa_delaware-county` | The [Site Structure Address Points service](https://gis.delcopa.gov/arcgis/rest/services/DataLayers_911/Site_Structure_Address_Points/FeatureServer/0) credits Delaware County Emergency Services but does not state reuse terms. The [ArcGIS item cited in the OpenAddresses definition](https://www.arcgis.com/home/item.html?id=319c975ff74d465abf8df89c3f4e6ef2) names Delaware County, **Ohio**, so it does not establish terms for the Pennsylvania data. | Confirm the correct license through the county's [data requests office](https://www.delcopa.gov/odmi/data-requests). |
| `us_pa_northampton-county` | The [Land Records address layer](https://gis.northamptoncounty.org/arcgisweb/rest/services/Assessment_Services/Land_Records_LGM/MapServer/6) has no reuse grant in its OpenAddresses definition. | Confirm the terms for this layer with [Northampton County GIS](https://www.norcopa.gov/gis). |
| `us_ga_fulton-county` | The [current Address Points dataset](https://gisdata.fultoncountyga.gov/datasets/fulcogis::address-points-1/about) contains data from multiple municipal publishers. Its custom terms disclaim accuracy and liability but do not explicitly grant redistribution. The [terms reported for the 2005 dataset](https://wiki.openstreetmap.org/wiki/Address_Data_Import_for_Fulton_County,_Georgia#Licensing) do not establish terms for the current combined layer. | Confirm coverage of the terms for the current layer, including the municipal contributions. |

## Before adding a layer

Confirm that the publisher permits downloading, transforming and redistributing the address data in a geocoding index, including for commercial use. Record the exact layer, terms and attribution in [`openaddresses_reviewed.tsv`](../../../src/bin/build_index/openaddresses_reviewed.tsv) if the builder cannot recognize the license from the source definition. Do not use `--openaddresses-any-licence` for a distributed build.

Build baseline and candidate indexes from otherwise identical sources. Check the admission receipt and attribution, then compare forward and reverse answers on the full index. Count a forward recovery only when it returns a confident, non-interpolated match for the requested indexed house, not a nearby house, street or ZIP centroid. Keep street and ZIP constraints unchanged and review any changes to existing matches separately from new coverage.
