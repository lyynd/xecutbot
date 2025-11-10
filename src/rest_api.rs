use std::sync::Weak;

use anyhow::{Error, Result};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use tower_http::catch_panic::CatchPanicLayer;

use crate::{VisitStatus, backend::Backend, config::RestApiConfig, utils::today};

#[derive(Clone)]
pub struct RestApi<B: Backend> {
    config: RestApiConfig,
    backend: Weak<B>,
}

impl<B: Backend + Clone + Send + Sync + 'static> RestApi<B> {
    pub fn new(config: RestApiConfig, backend: Weak<B>) -> Self {
        RestApi { config, backend }
    }

    pub async fn run(self) -> Result<()> {
        axum::serve(
            tokio::net::TcpListener::bind(&self.config.bind_address).await?,
            Self::router(self),
        )
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("listening for Ctrl-C should succeed");
        })
        .await?;
        Ok(())
    }

    fn router(self) -> Router<()> {
        Router::new()
            .route("/checked_in_count", get(Self::checked_in_count))
            .layer(CatchPanicLayer::new())
            .with_state(self)
    }

    async fn checked_in_count(
        State(state): State<RestApi<B>>,
    ) -> Result<impl IntoResponse, ApiError> {
        let today = today();
        let checked_in = state
            .backend
            .upgrade()
            .unwrap()
            .visits()
            .get_visits(today, today)
            .await?
            .iter()
            .filter(|v| v.status == VisitStatus::CheckedIn)
            .count();

        Ok(format!("{checked_in}"))
    }
}

// Make our own error that wraps `anyhow::Error`.
struct ApiError(Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        log::error!("REST API error: {:?}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong").into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for ApiError
where
    E: Into<Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
