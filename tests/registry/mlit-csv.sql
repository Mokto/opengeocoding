LOAD spatial;
CREATE TEMP VIEW source AS
    SELECT * FROM read_csv('raw/*_2025.csv', all_varchar := true, filename := true);
SELECT CASE WHEN count(DISTINCT filename) = 47 THEN true
    ELSE error('expected all 47 MLIT prefecture files') END FROM source;
COPY (
    WITH blocks AS (
        SELECT *, ST_Transform(
            ST_Point(try_cast("経度" AS DOUBLE), try_cast("緯度" AS DOUBLE)),
            'EPSG:4612', 'EPSG:4326', always_xy := true
        ) AS point
        FROM source
        WHERE "住居表示フラグ" = '1' AND "代表フラグ" = '1'
            AND "更新後履歴フラグ" <> '3'
            AND regexp_full_match("街区符号・地番", '[1-9][0-9]{0,3}')
    )
    SELECT regexp_extract(filename, '([0-9]{2})_2025.csv$', 1) AS pref_code,
        * EXCLUDE (filename, point), ST_Y(point) AS lat, ST_X(point) AS lon
    FROM blocks ORDER BY pref_code, "市区町村名", "大字・丁目名", "小字・通称名", "街区符号・地番", lat, lon
) TO 'mlit-blocks-2025-4326.csv' (HEADER, DELIMITER ',');
