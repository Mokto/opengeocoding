SET threads = 4;
COPY (
  WITH rows AS (
    SELECT *, CAST(regexp_extract(location_geometry_data, '^POINT \((\S+) (\S+)\)$', 2) AS DOUBLE) AS lat,
              CAST(regexp_extract(location_geometry_data, '^POINT \((\S+) (\S+)\)$', 1) AS DOUBLE) AS lon
    FROM read_csv('ryhti-open-address-2026-10-28.csv', header := true, all_varchar := true,
                  strict_mode := true)
    WHERE address_fin IS NOT NULL OR address_swe IS NOT NULL
  ),
  municipalities AS (
    SELECT m.sourceItem.code AS code, m.sourceItem.classificationItemNames[1].name AS name,
           s.classificationItemNames[1].name AS name_sv, m.targetItem.code AS region_code,
           m.targetItem.classificationItemNames[1].name AS region
    FROM read_json('kunta-maakunta-2026.json') AS m
    JOIN read_json('kunta-2026-sv.json') AS s ON s.code = m.sourceItem.code
  )
  SELECT min(address_key) AS address_key, municipality_number,
         any_value(m.name) AS municipality, any_value(m.name_sv) AS municipality_sv,
         any_value(m.region_code) AS region_code, any_value(m.region) AS region,
         coalesce(address_name_fin, '') AS street_fin, coalesce(address_name_swe, '') AS street_swe,
         coalesce(number_part_of_address_number, '') AS number,
         coalesce(subdivision_letter_of_address_number, '') AS letter,
         coalesce(number_part_of_address_number2, '') AS number2,
         coalesce(subdivision_letter_of_address_number2, '') AS letter2,
         coalesce(postal_code, '') AS postal_code,
         arg_min(coalesce(postal_office_fin, ''), address_key) AS postal_office_fin,
         arg_min(coalesce(postal_office_swe, ''), address_key) AS postal_office_swe,
         count(*) AS buildings,
         round(greatest((max(lat) - min(lat)) * 111320,
                        (max(lon) - min(lon)) * 111320 * cos(radians(min(lat))))) AS spread_m,
         arg_min(lat, address_key) AS lat, arg_min(lon, address_key) AS lon
  FROM rows JOIN municipalities AS m ON m.code = rows.municipality_number
  GROUP BY municipality_number, address_name_fin, address_name_swe, number_part_of_address_number,
           subdivision_letter_of_address_number, number_part_of_address_number2,
           subdivision_letter_of_address_number2, postal_code
  ORDER BY address_key
) TO 'ryhti-addresses-2026-10-28.csv.gz' (HEADER, COMPRESSION gzip);
