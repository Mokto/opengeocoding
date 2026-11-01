LOAD spatial;
-- WKB removes conflicting CRS type tags before UNION; each row keeps its explicit source CRS.
CREATE TEMP VIEW buildings AS
    SELECT 'EPSG:3183' AS crs, * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/0200QAQ/BYGNING.shp')
    UNION ALL SELECT 'EPSG:3182', * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/0600NUK/BYGNING.shp')
    UNION ALL SELECT 'EPSG:3182', * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/0800SIS/BYGNING.shp')
    UNION ALL SELECT 'EPSG:3182', * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/1000AAS/BYGNING.shp')
    UNION ALL SELECT 'EPSG:3182', * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/1200ILU/BYGNING.shp')
    UNION ALL SELECT 'EPSG:3184', * REPLACE (ST_GeomFromWKB(ST_AsWKB(geom)) AS geom) FROM ST_Read('raw/1800TAS/BYGNING.shp');
CREATE TEMP VIEW prepared AS
    WITH points AS (
        SELECT *, ST_PointOnSurface(ST_Force2D(geom)) AS anchor
        FROM buildings WHERE ST_IsValid(geom) AND ST_Area(geom) > 0
    ), transformed AS (
        SELECT *, ST_Transform(anchor, crs, 'EPSG:4326', always_xy := true) AS wgs84
        FROM points
    )
    SELECT lokalitet AS locality, crs,
        [ST_Y(wgs84), ST_X(wgs84)] AS point,
        [ST_X(anchor), ST_Y(anchor)] AS projected,
        sqrt(pow(ST_XMax(geom) - ST_XMin(geom), 2) + pow(ST_YMax(geom) - ST_YMin(geom), 2)) AS extent_m,
        struct_pack(lokal_id := lokal_id, lokalitet := lokalitet, maalepunkt := maalepunkt,
            bygningsty := bygningsty, bygningsbr := bygningsbr,
            created_da := created_da, B_nummer := B_nummer, last_edit := last_edit) AS record
    FROM transformed;
SELECT CASE WHEN count(*) > 0 AND bool_and(
    isfinite(p.point[1]) AND isfinite(p.point[2]) AND p.point[1] BETWEEN 59 AND 84 AND p.point[2] BETWEEN -74 AND -10
    AND ST_Contains(ST_Force2D(b.geom), ST_Point(p.projected[1], p.projected[2]))
) THEN true ELSE error('expected finite Greenlandic WGS84 coordinates and an interior building anchor') END
FROM prepared p JOIN buildings b ON p.locality = b.lokalitet AND p.record.lokal_id = b.lokal_id;
COPY (SELECT * FROM prepared ORDER BY locality, record.B_nummer, record.lokal_id)
    TO 'asiaq-buildings-4326.jsonl' (FORMAT JSON, ARRAY false);
