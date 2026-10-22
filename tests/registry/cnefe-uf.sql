SET threads = 4;
COPY (
  SELECT min(CAST(COD_UNICO_ENDERECO AS BIGINT)) AS id, any_value(COD_UF) AS uf_code,
         COD_MUNICIPIO AS municipality_code,
         NOM_TIPO_SEGLOGR AS street_type, coalesce(NOM_TITULO_SEGLOGR, '') AS street_title,
         NOM_SEGLOGR AS street_name, NUM_ENDERECO AS number,
         coalesce(DSC_MODIFICADOR, '') AS modifier,
         min(CEP) AS cep, count(DISTINCT CEP) AS ceps,
         min(DSC_LOCALIDADE) AS locality, count(DISTINCT DSC_LOCALIDADE) AS localities,
         count(*) AS rows, count(DISTINCT COD_UNICO_ENDERECO) AS units,
         min(NV_GEO_COORD) AS min_level, max(NV_GEO_COORD) AS max_level,
         count(*) FILTER (WHERE NUM_QUADRA = '0') AS rural_rows,
         string_agg(DISTINCT COD_ESPECIE, ' ' ORDER BY COD_ESPECIE) AS species,
         round(greatest((max(lat) - min(lat)) * 111320,
                        (max(lon) - min(lon)) * 111320 * cos(radians(avg(lat))))) AS spread_m,
         arg_min(lat, (CAST(COD_UNICO_ENDERECO AS BIGINT), lat, lon)) AS lat,
         arg_min(lon, (CAST(COD_UNICO_ENDERECO AS BIGINT), lat, lon)) AS lon
  FROM (
    SELECT *, CAST(LATITUDE AS DOUBLE) AS lat, CAST(LONGITUDE AS DOUBLE) AS lon
    FROM read_csv('/dev/stdin', delim := ';', quote := '', header := true, all_varchar := true)
  )
  GROUP BY COD_MUNICIPIO, NOM_TIPO_SEGLOGR, NOM_TITULO_SEGLOGR, NOM_SEGLOGR, NUM_ENDERECO, DSC_MODIFICADOR
) TO 'cnefe-uf.parquet';
