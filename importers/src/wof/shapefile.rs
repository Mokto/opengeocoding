use rayon::prelude::*;
use shapefile::dbase::FieldValue;

pub struct Shape {
    pub id: String,
    pub name: Option<String>,
    pub lat: Option<f64>,
    pub long: Option<f64>,
    pub shape: geo::MultiPolygon<f64>,
}

pub fn load_shapes(filename: &str) -> Result<Vec<Shape>, shapefile::Error> {
    let polygons =
        shapefile::read_as::<_, shapefile::Polygon, shapefile::dbase::Record>(filename).unwrap();

    let polygons = polygons
        .into_par_iter()
        .map(|(polygon, polygon_record)| {
            let id = match polygon_record.get("id").unwrap() {
                FieldValue::Character(id) => id.clone().unwrap(),
                _ => "".to_string(),
            };

            let geo_polygon: geo::MultiPolygon<f64> = polygon.into();

            Shape {
                id: id,
                name: None,
                lat: None,
                long: None,
                shape: geo_polygon,
            }
        })
        .collect::<Vec<_>>();

    Ok(polygons)
}
