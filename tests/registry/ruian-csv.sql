INSTALL spatial;
LOAD spatial;
SET threads = 4;
COPY (
  SELECT "Kód ADM" AS id, "Kód obce" AS municipality, "Název obce" AS city,
         "Kód MOMC" AS city_district, "Název části obce" AS part,
         "Název ulice" AS street, "Typ SO" AS number_type,
         "Číslo domovní" AS descriptive, "Číslo orientační" AS orientation,
         "Znak čísla orientačního" AS letter, "PSČ" AS postcode, region,
         round(ST_Y(point), 8) AS lat, round(ST_X(point), 8) AS lon
  FROM (
    SELECT *, ST_Transform(ST_Point(-CAST("Souřadnice Y" AS DOUBLE), -CAST("Souřadnice X" AS DOUBLE)),
                           'EPSG:5514', 'EPSG:4326', always_xy := true) AS point
    FROM read_csv('20260930_addresses-utf8.csv', delim := ';', all_varchar := true) AS a
    JOIN (
      SELECT DISTINCT OBEC_KOD AS municipality_code, VUSC_KOD AS region
      FROM read_csv('20260930_hierarchy.csv', delim := ';', all_varchar := true)
    ) AS h ON a."Kód obce" = h.municipality_code
    WHERE regexp_full_match("Kód ADM", '[0-9]+')
      AND "Souřadnice Y" IS NOT NULL AND "Souřadnice X" IS NOT NULL
  )
  ORDER BY CAST("Kód ADM" AS BIGINT)
) TO 'ruian-addresses-4326-2026-09-30.csv' (HEADER);
