INSTALL spatial;
LOAD spatial;
SET threads = 4;
COPY (
  SELECT kucni_broj_id AS id, opstina_maticni_broj AS municipality_id,
         opstina_ime AS municipality, opstina_ime_lat AS municipality_lat,
         naselje_maticni_broj AS settlement_id, naselje_ime AS settlement,
         naselje_ime_lat AS settlement_lat, ulica_maticni_broj AS street_id,
         ulica_ime AS street, ulica_ime_lat AS street_lat,
         kucni_broj AS number, kucni_broj_lat AS number_lat, tip_lat AS kind,
         place, place_lat,
         count(*) OVER (PARTITION BY place_lat, street_lat, number_lat) AS copies,
         round(ST_Y(point), 7) AS lat, round(ST_X(point), 7) AS lon
  FROM (
    -- A village without streets numbers its houses on a "street" named after the village, so its
    -- query names the municipality instead. Places are written without the register's qualifier
    -- ("БЕОГРАД (ЗЕМУН)", "ДОЛАЦ (НАСЕЉЕ)"); `copies` counts the addresses each query fits.
    SELECT *,
           regexp_replace(CASE WHEN ulica_ime = naselje_ime THEN opstina_ime ELSE naselje_ime END, ' \(.*\)$', '') AS place,
           regexp_replace(CASE WHEN ulica_ime = naselje_ime THEN opstina_ime_lat ELSE naselje_ime_lat END, ' \(.*\)$', '') AS place_lat,
           ST_Transform(ST_GeomFromText(wkt), 'EPSG:25834', 'EPSG:4326', always_xy := true) AS point
    FROM read_csv('kucni_broj.csv', header := true, all_varchar := true)
  )
  ORDER BY CAST(id AS BIGINT)
) TO 'rgz-addresses-4326-2025-06-10.csv' (HEADER);
