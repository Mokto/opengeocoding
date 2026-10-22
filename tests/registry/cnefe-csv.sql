SET threads = 4;
COPY (
  SELECT a.id, m.uf, a.municipality_code, m.municipality, a.street_type, a.street_title,
         a.street_name, a.number, a.modifier, a.cep, a.ceps, a.locality, a.localities, a.rows,
         a.units, a.min_level, a.max_level, a.rural_rows, a.species, a.spread_m, a.lat, a.lon
  FROM read_parquet('cnefe-*.parquet') AS a
  LEFT JOIN (
    SELECT CAST(id AS VARCHAR) AS code, nome AS municipality,
           "regiao-imediata"."regiao-intermediaria".UF.sigla AS uf
    FROM read_json('ibge-municipios-2026-10-22.json')
  ) AS m ON a.municipality_code = m.code
  ORDER BY a.id
) TO 'cnefe-addresses-2022.csv.gz' (HEADER, COMPRESSION gzip);
