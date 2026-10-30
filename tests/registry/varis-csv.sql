SET threads = 4;
COPY (
  WITH buildings AS (
    SELECT * FROM read_csv('aw_eka.csv', header := true, all_varchar := true, strict_mode := true)
  ),
  streets AS (
    SELECT * FROM read_csv('aw_iela.csv', header := true, all_varchar := true, strict_mode := true)
    WHERE STATUSS = 'EKS'
  ),
  villages AS (
    SELECT * FROM read_csv('aw_ciems.csv', header := true, all_varchar := true, strict_mode := true)
    WHERE STATUSS = 'EKS'
  ),
  parishes AS (
    SELECT * FROM read_csv('aw_pagasts.csv', header := true, all_varchar := true, strict_mode := true)
    WHERE STATUSS = 'EKS'
  ),
  cities AS (
    SELECT * FROM read_csv('aw_pilseta.csv', header := true, all_varchar := true, strict_mode := true)
    WHERE STATUSS = 'EKS'
  ),
  municipalities AS (
    SELECT * FROM read_csv('aw_novads.csv', header := true, all_varchar := true, strict_mode := true)
    WHERE STATUSS = 'EKS'
  ),
  placed AS (
    SELECT b.*, s.NOSAUKUMS AS IELA,
           coalesce(s.VKUR_CD, b.VKUR_CD) AS PLACE_CD, coalesce(s.VKUR_TIPS, b.VKUR_TIPS) AS PLACE_TIPS
    FROM buildings AS b
    LEFT JOIN streets AS s ON b.VKUR_TIPS = '107' AND s.KODS = b.VKUR_CD
    WHERE b.STATUSS = 'EKS' AND b.APSTIPR = 'Y' AND b.PLAN_ADR = 'N'
      AND (b.VKUR_TIPS <> '107' OR s.KODS IS NOT NULL)
  )
  SELECT a.KODS, coalesce(m.NOSAUKUMS, '') AS NOVADS, coalesce(m.SORT_NOS, '') AS NOVADS_PILNS,
         coalesce(p.NOSAUKUMS, '') AS PAGASTS, coalesce(p.SORT_NOS, '') AS PAGASTS_PILNS,
         coalesce(t.NOSAUKUMS, '') AS PILSETA, coalesce(v.NOSAUKUMS, '') AS CIEMS,
         coalesce(v.KODS, t.KODS, '') AS VIETA_CD, coalesce(a.IELA, '') AS IELA, a.NOSAUKUMS,
         a.ATRIB, a.FOR_BUILD, a.DAT_SAK, a.STD, a.KOORD_X, a.KOORD_Y, a.DD_N, a.DD_E
  FROM placed AS a
  LEFT JOIN cities AS t ON a.PLACE_TIPS = '104' AND t.KODS = a.PLACE_CD
  LEFT JOIN villages AS v ON a.PLACE_TIPS = '106' AND v.KODS = a.PLACE_CD
  LEFT JOIN parishes AS p ON p.KODS = CASE a.PLACE_TIPS WHEN '105' THEN a.PLACE_CD WHEN '106' THEN v.VKUR_CD END
  LEFT JOIN municipalities AS m ON m.KODS = coalesce(p.VKUR_CD, t.VKUR_CD)
  WHERE CASE a.PLACE_TIPS
          WHEN '104' THEN t.KODS IS NOT NULL AND (t.VKUR_TIPS = '101' OR m.KODS IS NOT NULL)
          WHEN '105' THEN p.KODS IS NOT NULL AND m.KODS IS NOT NULL
          WHEN '106' THEN v.KODS IS NOT NULL AND p.KODS IS NOT NULL AND m.KODS IS NOT NULL
          ELSE false
        END
  ORDER BY CAST(a.KODS AS BIGINT)
) TO 'varis-addresses-2026-10-29.csv' (HEADER);
