use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::info;

use crate::api::{APIResponse, QueryParams};
use crate::db::Database;

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_LIMIT: u32 = 50;

#[derive(Debug)]
pub struct HandlerParams {
    pub q: Option<String>,
    pub page: u32,
    pub limit: u32,
    pub offset: u32,
}

impl QueryParams {
    pub fn into_handler_params(self) -> HandlerParams {
        let page = self.page.unwrap_or(DEFAULT_PAGE).min(1);
        let limit = self
            .limit
            .unwrap_or(DEFAULT_LIMIT)
            .max(0)
            .min(DEFAULT_LIMIT);

        HandlerParams {
            q: self.q,
            page: page,
            limit: limit,
            offset: (page - 1) * limit,
        }
    }
}

pub async fn healthcheck() -> impl IntoResponse {
    info!("got healthcheck request");
    Json(APIResponse::new(Some("ok"), None))
}

pub async fn get_books(State(db): State<Arc<Database>>, Query(qp): Query<QueryParams>) -> Response {
    let hp = qp.into_handler_params();
    let db_call = db.get_books(hp).await;

    if let Err(e) = db_call {
        tracing::info!("failed to get books. db_error: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(APIResponse::new(Some("failed to get books"), None)),
        )
            .into_response();
    }

    tracing::info!("got books");
    let response = APIResponse::new(Some("got books"), db_call.ok());

    (StatusCode::OK, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    fn test_params_deserialize() {
        // let params = QueryParams {
        //     q: Some("test".to_string()),
        // };
        // assert_eq!(params.q, Some("test".to_string()));
    }
}
