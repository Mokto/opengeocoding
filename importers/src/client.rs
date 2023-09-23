// use hello_world::HelloRequest;

use opengeocoding::open_geocoding_internal_client::OpenGeocodingInternalClient;

pub mod opengeocoding {
    tonic::include_proto!("opengeocoding");
}

pub struct OpenGeocodingApiClient {
    client: OpenGeocodingInternalClient<tonic::transport::Channel>,
}

impl OpenGeocodingApiClient {
    pub async fn new() -> Result<OpenGeocodingApiClient, Box<dyn std::error::Error>> {
        let client: OpenGeocodingInternalClient<tonic::transport::Channel> =
            OpenGeocodingInternalClient::connect("http://127.0.0.1:8091").await?;
        Ok(OpenGeocodingApiClient { client })
    }

    pub async fn run_query(&mut self, query: String) -> Result<String, tonic::Status> {
        let request: tonic::Request<opengeocoding::RunQueryRequest> =
            tonic::Request::new(opengeocoding::RunQueryRequest { query: query });
        let response = self.client.run_query(request);

        return Ok(response.await?.into_inner().result);
    }

    pub async fn run_background_query(&mut self, query: String) -> Result<(), tonic::Status> {
        let request =
            tonic::Request::new(opengeocoding::RunBackgroundQueryRequest { query: query });
        let response = self.client.run_background_query(request);

        response.await?;

        Ok(())
    }

    pub async fn insert_locations(
        &mut self,
        locations: Vec<opengeocoding::Location>,
    ) -> Result<(), tonic::Status> {
        for chunk in locations.chunks(10000) {
            let request = tonic::Request::new(opengeocoding::InsertLocationsRequest {
                locations: chunk.to_vec(),
            });
            let response = self.client.insert_locations(request);

            response.await?;
        }

        Ok(())
    }
}
