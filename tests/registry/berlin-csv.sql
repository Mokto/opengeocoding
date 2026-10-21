INSTALL spatial;
LOAD spatial;
SET threads = 4;
-- This extract has no header. Column 0 is an empty record-kind field; columns 1–20
-- are HK-DE v5.2 (oid through postcode). A trailing extract date follows the postal-name
-- fields. info-be.txt says EPSG:25833, so the national description's zone 32 does not apply.
COPY (
  SELECT id, quality, bezirk, coalesce(ortsteil, '') AS ortsteil, street_key, street,
         number, coalesce(addition, '') AS addition, coalesce(postcode, '') AS postcode,
         round(ST_Y(point), 8) AS lat, round(ST_X(point), 8) AS lon
  FROM (
    SELECT column01 AS id, column02 AS quality, column10 AS bezirk, column12 AS ortsteil,
           column13 AS street_key, column14 AS street, column15 AS number,
           column16 AS addition, column20 AS postcode,
           ST_Transform(ST_Point(CAST(column18 AS DOUBLE), CAST(column19 AS DOUBLE)),
                        'EPSG:25833', 'EPSG:4326', always_xy := true) AS point
    FROM read_csv('adressen-be_mitPLZ.txt', delim := ';', header := false, all_varchar := true)
    WHERE column17 = '33' AND column18 IS NOT NULL AND column19 IS NOT NULL
  )
  ORDER BY id
) TO 'berlin-addresses-4326-2026-03-23.csv' (HEADER);
