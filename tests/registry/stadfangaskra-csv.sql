SET threads = 4;
COPY (
  WITH addresses AS (
    SELECT * FROM read_csv('stadfangaskra-wfs.csv', header := true, all_varchar := true, strict_mode := true)
  ),
  postcodes AS (
    SELECT trim("Póstnúmer") AS POSTNR, trim("Staður") AS STADUR
    FROM read_xlsx('postnumeraskra_2024.xlsx', all_varchar := true)
    WHERE "Póstnúmer" IS NOT NULL
  ),
  -- IS 50V numbers the Borgarbyggð that absorbed Skorradalshreppur on 2026-06-01 with
  -- Skorradalshreppur's 3506; the address register keeps Borgarbyggð's 3609 for both.
  municipalities AS (
    SELECT DISTINCT CASE lpad(nrsveitarfelags, 4, '0') WHEN '3506' THEN '3609' ELSE lpad(nrsveitarfelags, 4, '0') END AS SVFNR,
           sveitarfelag AS SVEITARFELAG
    FROM read_csv('sveitarfelog-wfs.csv', header := true, all_varchar := true, strict_mode := true)
  )
  SELECT a.HNITNUM, a.HEINUM, a.LANDNR, a.SVFNR,
         coalesce(m.SVEITARFELAG, error('no IS 50V municipality ' || a.SVFNR)) AS SVEITARFELAG,
         coalesce(a.POSTNR, '') AS POSTNR,
         CASE WHEN a.POSTNR IS NULL THEN '' ELSE coalesce(p.STADUR, error('no post town ' || a.POSTNR)) END AS STADUR,
         coalesce(a.HEITI_NF, '') AS HEITI_NF, coalesce(a.HEITI_TGF, '') AS HEITI_TGF,
         coalesce(a.HUSNR, '') AS HUSNR, coalesce(a.BOKST, '') AS BOKST, coalesce(a.VIDSK, '') AS VIDSK,
         coalesce(a.SERHEITI, '') AS SERHEITI, coalesce(a.TEGHNIT, '') AS TEGHNIT,
         coalesce(a.YFIRFARID, '') AS YFIRFARID, coalesce(a.DAGS_LEIDR, '') AS DAGS_LEIDR,
         a.VEF_BIRTING, a.HNIT, a.N_HNIT_WGS84, a.E_HNIT_WGS84
  FROM addresses AS a
  LEFT JOIN municipalities AS m USING (SVFNR)
  LEFT JOIN postcodes AS p USING (POSTNR)
  ORDER BY CAST(a.HNITNUM AS BIGINT)
) TO 'stadfangaskra-2026-10-30.csv' (HEADER);
