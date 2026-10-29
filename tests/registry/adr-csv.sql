SET threads = 4;
COPY (
  WITH points AS (
    SELECT unnest(features, recursive := true)
    FROM read_json('adr_gra_adresai_LT.json', maximum_object_size := 600000000,
                   format := 'unstructured')
  ),
  addresses AS (
    SELECT * FROM read_csv('adr_stat_lr.csv', delim := '|', header := true, all_varchar := true,
                           strict_mode := true)
  ),
  settlements AS (
    SELECT * FROM read_csv('adr_gyvenamosios_vietoves.csv', delim := '|', header := true,
                           all_varchar := true, strict_mode := true)
  ),
  streets AS (
    SELECT * FROM read_csv('adr_gatves.csv', delim := '|', header := true, all_varchar := true,
                           strict_mode := true)
  ),
  municipalities AS (
    SELECT * FROM read_csv('adr_savivaldybes.csv', delim := '|', header := true,
                           all_varchar := true, strict_mode := true)
  ),
  counties AS (
    SELECT * FROM read_csv('adr_apskritys.csv', delim := '|', header := true, all_varchar := true,
                           strict_mode := true)
  )
  SELECT a.AOB_KODAS, c.VARDAS_K AS ADM_VARDAS_K, a.SAV_KODAS, m.VARDAS_K AS SAV_VARDAS_K,
         a.GYV_KODAS, g.TIPAS AS GYV_TIPAS, g.TIPO_SANTRUMPA AS GYV_SANTRUMPA,
         g.VARDAS_K AS GYV_VARDAS_K, g.VARDAS AS GYV_VARDAS, coalesce(a.GAT_KODAS, '') AS GAT_KODAS,
         coalesce(s.TIPO_SANTRUMPA, '') AS GAT_SANTRUMPA, coalesce(s.VARDAS_K, '') AS GAT_VARDAS_K,
         a.NR, coalesce(a.KORPUSO_NR, '') AS KORPUSO_NR, coalesce(a.PASTO_KODAS, '') AS PASTO_KODAS,
         a.AOB_NUO, p.N_KOORD, p.E_KOORD
  FROM addresses AS a
  JOIN points AS p ON CAST(p.AOB_KODAS AS VARCHAR) = a.AOB_KODAS
  JOIN settlements AS g ON g.GYV_KODAS = a.GYV_KODAS AND g.SAV_KODAS = a.SAV_KODAS
  JOIN municipalities AS m ON m.SAV_KODAS = a.SAV_KODAS
  JOIN counties AS c ON c.ADM_KODAS = m.ADM_KODAS
  LEFT JOIN streets AS s ON s.GAT_KODAS = a.GAT_KODAS
  WHERE CAST(p.GYV_KODAS AS VARCHAR) = a.GYV_KODAS
    AND coalesce(CAST(nullif(p.GAT_KODAS, 0) AS VARCHAR), '') = coalesce(a.GAT_KODAS, '')
    AND (a.GAT_KODAS IS NULL OR s.GYV_KODAS = a.GYV_KODAS)
  ORDER BY CAST(a.AOB_KODAS AS BIGINT)
) TO 'adr-addresses-2026-10-01.csv' (HEADER);
