use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;

use super::AppState;
use crate::error::Error;

pub async fn require_admin(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, Error> {
    let presented = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    match presented {
        Some(token) if constant_time_eq(token.as_bytes(), state.config.admin_token.expose().as_bytes()) => {
            Ok(next.run(request).await)
        }
        _ => Err(Error::Unauthorized),
    }
}

/// Compares without stopping at the first difference, so response timing does not
/// reveal how much of a guessed token was correct.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0
}

#[cfg(test)]
mod tests {
    use super::constant_time_eq;

    #[test]
    fn compares_whole_tokens() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"tokem"));
        assert!(!constant_time_eq(b"token", b"token-and-more"));
        assert!(!constant_time_eq(b"", b"token"));
    }
}
