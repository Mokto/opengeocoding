SET threads = 4;
COPY (
  SELECT id, cve_ent, entidad, cve_mun, municipio, cve_loc, trim(localidad) AS localidad,
         coalesce(tipo_asent, '') AS tipo_asent, coalesce(nomb_asent, '') AS nomb_asent,
         coalesce(tipo_vial, '') AS tipo_vial, coalesce(nom_vial, '') AS nom_vial,
         coalesce(numero_ext, '') AS numero_ext, coalesce(letra_ext, '') AS letra_ext,
         coalesce(cod_postal, '') AS cod_postal, per_ocu, tipoUniEco AS tipo_uni_eco,
         tipoCenCom IS NOT NULL AS centro_comercial, fecha_alta,
         count(*) OVER address AS establecimientos,
         count(DISTINCT coalesce(cod_postal, '')) OVER address AS codigos_postales,
         round(greatest((max(lat) OVER address - min(lat) OVER address) * 111320,
                        (max(lon) OVER address - min(lon) OVER address) * 111320
                          * cos(radians(lat)))) AS dispersion_m,
         latitud, longitud
  FROM (
    SELECT *, CAST(latitud AS DOUBLE) AS lat, CAST(longitud AS DOUBLE) AS lon
    FROM read_csv('csv/denue_inegi_*.csv', header := true, all_varchar := true,
                  quote := '"', escape := '"', strict_mode := true)
  )
  WINDOW address AS (PARTITION BY cve_ent, cve_mun, cve_loc, tipo_vial, nom_vial, numero_ext,
                                  letra_ext)
  ORDER BY CAST(id AS BIGINT)
) TO 'denue-addresses-05-2026.csv.gz' (HEADER, COMPRESSION gzip);
