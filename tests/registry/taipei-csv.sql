LOAD spatial;
COPY (
    WITH addresses AS (
        SELECT *, ST_Transform(
            ST_Point(try_cast("橫座標" AS DOUBLE), try_cast("縱座標" AS DOUBLE)),
            'EPSG:3826', 'EPSG:4326', always_xy := true
        ) AS point
        FROM read_csv('taipei.csv', all_varchar := true)
    )
    SELECT * EXCLUDE (point), ST_Y(point) AS lat, ST_X(point) AS lon FROM addresses
) TO 'taipei-4326-2026-10-19.csv' (HEADER, DELIMITER ',');
