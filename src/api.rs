use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::{OpenApi, ToSchema};

use crate::client::{self, DepError};

pub const SERVICE: &str = "srvcs-log10";
pub const CONCERN: &str = "arithmetic: base-10 logarithm";
pub const DEPENDS_ON: &[&str] = &["srvcs-isnumber"];

/// Dependency endpoints, injected as router state so tests can point them at
/// mock services.
#[derive(Clone)]
pub struct Deps {
    pub isnumber_url: String,
}

#[derive(Serialize, ToSchema)]
pub struct Info {
    pub service: &'static str,
    pub concern: &'static str,
    pub depends_on: Vec<&'static str>,
}

/// `GET /` — service identity (srvcs service standard).
#[utoipa::path(get, path = "/", responses((status = 200, body = Info)))]
pub async fn index() -> Json<Info> {
    Json(Info {
        service: SERVICE,
        concern: CONCERN,
        depends_on: DEPENDS_ON.to_vec(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct EvalRequest {
    #[schema(value_type = Object)]
    pub value: Value,
}

#[derive(Serialize, ToSchema)]
pub struct Log10Response {
    #[schema(value_type = Object)]
    pub value: Value,
    pub result: f64,
}

/// The single concern: the base-10 logarithm of a strictly positive real
/// number.
///
/// Returns `None` for a non-positive input (`value <= 0.0`), where the base-10
/// logarithm is not defined over the reals. Otherwise returns
/// `Some(value.log10())`, e.g. `log10(1000) == 3.0`, `log10(1) == 0.0`.
pub fn log10(f: f64) -> Option<f64> {
    if f <= 0.0 {
        None
    } else {
        Some(f.log10())
    }
}

fn ok(value: Value, result: f64) -> Response {
    (
        StatusCode::OK,
        Json(json!({ "value": value, "result": result })),
    )
        .into_response()
}

fn invalid(reason: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": reason })),
    )
        .into_response()
}

fn degraded(dependency: &str) -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "dependency unavailable", "dependency": dependency })),
    )
        .into_response()
}

/// Forward a dependency's response verbatim (used to propagate `422` for invalid
/// input, so log10 reports the same rejection its dependency did).
fn forward(status: u16, body: Value) -> Response {
    let code = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, Json(body)).into_response()
}

/// Validate `value` is a number by asking `srvcs-isnumber`, mapping its
/// failures to the response this service should return.
async fn ask_is_number(url: &str, value: &Value, dependency: &str) -> Result<(), Response> {
    match client::call(url, &json!({ "value": value })).await {
        Err(DepError::Unreachable) => Err(degraded(dependency)),
        Ok((200, body)) => {
            let is_number = body.get("result").and_then(Value::as_bool).unwrap_or(false);
            if is_number {
                Ok(())
            } else {
                Err(invalid("value is not a number"))
            }
        }
        // Invalid input propagates from the leaf dependency; forward it.
        Ok((422, body)) => Err(forward(422, body)),
        Ok(_) => Err(degraded(dependency)),
    }
}

/// `POST /` — compute `log10(value)`.
///
/// Input validation is delegated to `srvcs-isnumber` over HTTP (the single
/// source of truth for "is this a number"). Both integers and floats are valid
/// input: this is a floating-point service and the result is an `f64`. The
/// base-10 logarithm is undefined for non-positive inputs, so `value <= 0.0`
/// yields a `422` domain error. If the dependency is unreachable, this service
/// reports itself degraded rather than guessing.
#[utoipa::path(
    post,
    path = "/",
    request_body = EvalRequest,
    responses(
        (status = 200, body = Log10Response),
        (status = 422, description = "value is not a number, or is non-positive"),
        (status = 500, description = "value passed validation but is not representable as a number"),
        (status = 503, description = "a dependency is unavailable")
    )
)]
pub async fn evaluate(State(deps): State<Deps>, Json(req): Json<EvalRequest>) -> Response {
    // 1. Delegate "is this a number" to srvcs-isnumber.
    if let Err(resp) = ask_is_number(&deps.isnumber_url, &req.value, "srvcs-isnumber").await {
        return resp;
    }

    // 2. Coerce to f64; accept both integers and floats.
    let Some(f) = req.value.as_f64() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "value validated as a number but is not representable as f64" })),
        )
            .into_response();
    };

    // 3. log10 is undefined for non-positive inputs.
    match log10(f) {
        Some(result) => ok(req.value, result),
        None => invalid("logarithm of a non-positive number"),
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(index, evaluate),
    components(schemas(Info, EvalRequest, Log10Response))
)]
pub struct ApiDoc;

/// Serve OpenAPI document
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_documents_routes() {
        let doc = ApiDoc::openapi();
        let root = doc.paths.paths.get("/").expect("path / present");
        assert!(root.get.is_some());
        assert!(root.post.is_some());
    }

    #[test]
    fn log10_of_powers_of_ten() {
        assert!((log10(1000.0).unwrap() - 3.0).abs() < 1e-9);
        assert!((log10(1.0).unwrap() - 0.0).abs() < 1e-9);
        assert!((log10(100.0).unwrap() - 2.0).abs() < 1e-9);
        assert!((log10(0.1).unwrap() - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn log10_of_arbitrary_value() {
        // log10(4) ~= 0.602059991327962...
        assert!((log10(4.0).unwrap() - 0.602_059_991_327_962_4).abs() < 1e-9);
    }

    #[test]
    fn log10_of_non_positive_is_undefined() {
        assert!(log10(0.0).is_none());
        assert!(log10(-1.0).is_none());
        assert!(log10(-0.0001).is_none());
    }

    #[tokio::test]
    async fn index_reports_dependency() {
        let Json(info) = index().await;
        assert_eq!(info.service, "srvcs-log10");
        assert_eq!(info.concern, "arithmetic: base-10 logarithm");
        assert_eq!(info.depends_on, vec!["srvcs-isnumber"]);
    }
}
