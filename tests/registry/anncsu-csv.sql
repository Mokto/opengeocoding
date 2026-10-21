SET threads = 4;
COPY (
  SELECT a.PROGRESSIVO_ACCESSO AS id, a.PROGRESSIVO_NAZIONALE AS street_id,
         a.CODICE_COMUNE AS municipality_code, m.COMUNE_IT AS municipality,
         m.COMUNE AS municipality_names, m.SIGLA_AUTOMOBILISTICA AS province, m.DEN_REG AS region,
         a.ODONIMO AS street, a.DIZIONE_LINGUA1 AS street_lang1, a.DIZIONE_LINGUA2 AS street_lang2,
         a."LOCALITA'" AS locality, a.CIVICO AS number, a.ESPONENTE AS exponent,
         a.SPECIFICITA AS specificity, a.METRICO AS metric, a.PROGRESSIVO_SNC AS snc,
         a.METODO AS method, a.copies,
         CAST(replace(a.COORD_Y_COMUNE, ',', '.') AS DOUBLE) AS lat,
         CAST(replace(a.COORD_X_COMUNE, ',', '.') AS DOUBLE) AS lon
  FROM (
    SELECT *, count(*) OVER (PARTITION BY CODICE_COMUNE, PROGRESSIVO_NAZIONALE, CIVICO,
                             coalesce(ESPONENTE, ''), coalesce(SPECIFICITA, ''),
                             coalesce(METRICO, ''), coalesce(PROGRESSIVO_SNC, '')) AS copies
    FROM read_csv('INDIR_ITA_20260915.csv', delim := ';', quote := '', header := true, all_varchar := true)
  ) AS a
  JOIN (
    SELECT unnest(resultset, recursive := true)
    FROM read_json('situas-comuni-2026-09-15.json', maximum_object_size := 67108864)
  ) AS m ON a.CODICE_COMUNE = m.COD_CATASTO
  WHERE a.COORD_X_COMUNE IS NOT NULL AND a.COORD_Y_COMUNE IS NOT NULL
  ORDER BY CAST(a.PROGRESSIVO_ACCESSO AS BIGINT)
) TO 'anncsu-addresses-2026-09-15.csv' (HEADER);
