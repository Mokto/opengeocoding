use crate::{
    matching::search_override,
    metrics::METRICS,
    proto::{
        common::{EmptyRequest, EmptyResponse},
        nominatim_geolocation::*,
    },
    regions::Regions,
    resolver::Resolver,
};
use anyhow::Result;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceResult};

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
        let service = "nominatim_geolocation.NominatimGeolocation";
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

impl NominatimGeolocation for Service {
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
                let regions = self
                    .regions
                    .reformat(&self.resolver, &details.region_codes)
                    .await?;
                details.regions.extend(regions);
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
            if !request.lat.is_finite()
                || !request.lon.is_finite()
                || !(-90.0..=90.0).contains(&request.lat)
                || !(-180.0..=180.0).contains(&request.lon)
            {
                return Err(ConnectError::invalid_argument(
                    "Enter valid latitude and longitude coordinates.",
                )
                .into());
            }
            let Some(address) = self.resolver.reverse(request.lat, request.lon).await? else {
                return Ok(RegionsResponse::default());
            };
            let Some(details) = address.address_details.as_option() else {
                return Ok(RegionsResponse::default());
            };
            Ok(RegionsResponse {
                regions: self
                    .regions
                    .reformat(&self.resolver, &details.region_codes)
                    .await?,
                ..Default::default()
            })
        })
        .await
    }

    async fn refresh_region_map<'a>(
        &'a self,
        ctx: RequestContext,
        _: ServiceRequest<'_, EmptyRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<EmptyResponse> + Send + use<'a>> {
        self.run(&ctx, "RefreshRegionMap", async {
            self.regions.refresh_map().await?;
            Ok(EmptyResponse::default())
        })
        .await
    }

    async fn get_region_map<'a>(
        &'a self,
        ctx: RequestContext,
        _: ServiceRequest<'_, EmptyRequest>,
    ) -> ServiceResult<impl connectrpc::Encodable<RegionMap> + Send + use<'a>> {
        self.run(&ctx, "GetRegionMap", async {
            Ok(RegionMap {
                region_map: self.regions.read_map().await?,
                ..Default::default()
            })
        })
        .await
    }
}
