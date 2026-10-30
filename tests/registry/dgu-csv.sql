SET threads = 4;
COPY (
  WITH points AS (
    SELECT * EXCLUDE (punto_wkb)
    FROM read_csv('utf8/*.csv', header := true, all_varchar := true, strict_mode := true,
                  delim := ',', quote := '"', escape := '"')
  )
  SELECT *,
         count(*) OVER (PARTITION BY departamento, localidad, nombre_via, num_puerta, letra_puerta) AS copies
  FROM points
  ORDER BY ALL
) TO 'dgu-addresses-2026-10-01.csv' (HEADER);
