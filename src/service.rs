use crate::{
    matching::search_override, metrics::METRICS, proto::opengeocoding::*, regions::Regions,
    resolver::Resolver,
};
use anyhow::Result;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};

const SUGGEST_DEFAULT: usize = 5;
const SUGGEST_MAX: usize = 15;
fn valid_coordinates(lat: f32, lon: f32) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

const REVERSE_RADIUS_DEFAULT: u32 = 500;
const REVERSE_RADIUS_MAX: u32 = 2000;
const REVERSE_DEFAULT: usize = 1;
const REVERSE_MAX: usize = 10;

pub struct Service {
    pub resolver: Resolver,
    pub regions: Regions,
}

impl Service {
    async fn run<T>(
        &self,
        ctx: &RequestContext,
        method: &str,
        operation: impl Future<Output = Result<T>>,
    ) -> ServiceResult<T> {
        let service = "opengeocoding.Geolocation";
        let organization = ctx
            .headers()
            .get("x-organization-id")
            .and_then(|value| value.to_str().ok())
            .filter(|s| !s.is_empty())
            .unwrap_or("anonymous");
        METRICS.started.with_label_values(&[service, method]).inc();
        METRICS
            .requests
            .with_label_values(&[service, method, organization])
            .inc();
        let _timer = METRICS
            .duration
            .with_label_values(&[service, method])
            .start_timer();
        match operation.await {
            Ok(value) => {
                METRICS
                    .handled
                    .with_label_values(&[service, method, "OK"])
                    .inc();
                Response::ok(value)
            }
            Err(error) => {
                let error = match error.downcast::<ConnectError>() {
                    Ok(error) => error,
                    Err(error) => {
                        tracing::error!(method, %error, "Geolocation request failed");
                        sentry::capture_error::<dyn std::error::Error>(error.as_ref());
                        ConnectError::unknown("The geolocation request could not be completed.")
                    }
                };
                METRICS
                    .handled
                    .with_label_values(&[service, method, &error.code.as_str().to_uppercase()])
                    .inc();
                Err(error)
            }
        }
    }
}

impl Geolocation for Service {
    async fn search_address<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SearchAddressTask>,
    ) -> ServiceResult<impl connectrpc::Encodable<FullAddress> + Send + use<'a>> {
        self.run(&ctx, "SearchAddress", async {
            if request.address_string.contains('\0') {
                return Err(ConnectError::invalid_argument(
                    "The address contains an invalid character.",
                )
                .into());
            }
            if let Some(overridden) = search_override(request.address_string) {
                return Ok(overridden);
            }
            let mut address = self.resolver.resolve(request.address_string).await?;
            if let Some(details) = address.address_details.as_option_mut() {
                details
                    .regions
                    .extend(self.regions.reformat(&details.region_codes));
            }
            Ok(address)
        })
        .await
    }

    async fn resolve_region<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, GeoCoordinates>,
    ) -> ServiceResult<impl connectrpc::Encodable<RegionsResponse> + Send + use<'a>> {
        self.run(&ctx, "ResolveRegion", async {
            if !valid_coordinates(request.lat, request.lon) {
                return Err(ConnectError::invalid_argument(
                    "Enter valid latitude and longitude coordinates.",
                )
                .into());
            }
            let Some(address) = self
                .resolver
                .reverse_region(request.lat, request.lon)
                .await?
            else {
                return Ok(RegionsResponse::default());
            };
            let Some(details) = address.address_details.as_option() else {
                return Ok(RegionsResponse::default());
            };
            Ok(RegionsResponse {
                regions: self.regions.reformat(&details.region_codes),
                ..Default::default()
            })
        })
        .await
    }

    async fn suggest_address<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, SuggestAddressRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<SuggestAddressResponse> + Send + use<'a>> {
        self.run(&ctx, "SuggestAddress", async {
            if request.text.contains('\0') {
                return Err(ConnectError::invalid_argument(
                    "The text contains an invalid character.",
                )
                .into());
            }
            let country = request.country_code;
            if !(country.is_empty()
                || country.len() == 2 && country.bytes().all(|b| b.is_ascii_alphabetic()))
            {
                return Err(ConnectError::invalid_argument(
                    "The country code must be two letters or empty.",
                )
                .into());
            }
            let limit = match request.limit {
                0 => SUGGEST_DEFAULT,
                n => (n as usize).min(SUGGEST_MAX),
            };
            let found = self.resolver.suggest(request.text, limit, country).await?;
            Ok(SuggestAddressResponse {
                suggestions: found
                    .into_iter()
                    .map(|s| Suggestion {
                        id: s.id,
                        display_name: s.display_name,
                        geo_coordinates: GeoCoordinates {
                            lat: s.y as f32,
                            lon: s.x as f32,
                            bounding_box: s
                                .bbox
                                .map(|[min_lon, min_lat, max_lon, max_lat]| BoundingBox {
                                    min_lat,
                                    max_lat,
                                    min_lon,
                                    max_lon,
                                    ..Default::default()
                                })
                                .into(),
                            ..Default::default()
                        }
                        .into(),
                        r#type: s.kind.into(),
                        country_code: s.country_code,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })
        })
        .await
    }

    async fn reverse_geocode<'a>(
        &'a self,
        ctx: RequestContext,
        request: ServiceRequest<'_, ReverseGeocodeRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<ReverseGeocodeResponse> + Send + use<'a>> {
        self.run(&ctx, "ReverseGeocode", async {
            if !valid_coordinates(request.lat, request.lon) {
                return Err(ConnectError::invalid_argument(
                    "Enter valid latitude and longitude coordinates.",
                )
                .into());
            }
            let radius = match request.radius_m {
                0 => REVERSE_RADIUS_DEFAULT,
                n => n.min(REVERSE_RADIUS_MAX),
            };
            let size = match request.size {
                0 => REVERSE_DEFAULT,
                n => (n as usize).min(REVERSE_MAX),
            };
            let mut results = self
                .resolver
                .reverse_search(request.lat, request.lon, radius as f64, size)
                .await?;
            for result in &mut results {
                if let Some(details) = result
                    .address
                    .as_option_mut()
                    .and_then(|a| a.address_details.as_option_mut())
                {
                    details
                        .regions
                        .extend(self.regions.reformat(&details.region_codes));
                }
            }
            Ok(ReverseGeocodeResponse {
                results,
                ..Default::default()
            })
        })
        .await
    }

    async fn list_regions<'a>(
        &'a self,
        ctx: RequestContext,
        _: ServiceRequest<'_, EmptyRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<RegionsResponse> + Send + use<'a>> {
        self.run(&ctx, "ListRegions", async {
            Ok(RegionsResponse {
                regions: self.regions.list().to_vec(),
                ..Default::default()
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::valid_coordinates;

    #[test]
    fn coordinates_must_be_on_the_globe() {
        for (lat, lon) in [(0., 0.), (90., 180.), (-90., -180.), (55.676098, 12.568337)] {
            assert!(valid_coordinates(lat, lon), "{lat} {lon}");
        }
        for (lat, lon) in [
            (90.01, 0.),
            (-90.01, 0.),
            (0., 180.01),
            (0., -180.01),
            (f32::NAN, 0.),
            (0., f32::NAN),
            (f32::INFINITY, 0.),
            (0., f32::NEG_INFINITY),
            (f32::MAX, f32::MAX),
        ] {
            assert!(!valid_coordinates(lat, lon), "{lat} {lon}");
        }
    }
}
