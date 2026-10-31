LOAD spatial;
COPY (
    WITH addresses AS (
        SELECT *, ST_Transform(
            ST_Point(try_cast("橫座標[E 坐標]" AS DOUBLE), try_cast("縱座標[N 坐標]" AS DOUBLE)),
            'EPSG:3826', 'EPSG:4326', always_xy := true
        ) AS point
        FROM read_csv('tainan-114.csv', all_varchar := true)
    )
    SELECT * EXCLUDE (point), ST_Y(point) AS lat, ST_X(point) AS lon FROM addresses
) TO 'tainan-114-4326.csv' (HEADER, DELIMITER ',');
