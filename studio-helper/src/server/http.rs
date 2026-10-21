//! HTTP framing and wire responses; authorization remains in the route dispatcher.
use axum::body::HttpBody;

use super::*;
/// Returns the sole UTF-8 value named `name` in `headers`, or `None` when missing,
/// non-text or repeated. Security-sensitive callers must treat absence as rejection.
pub(super) fn one<'a>(
    headers: &'a HeaderMap,
    name: &str,
) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() { None } else { Some(first) }
}
/// Returns whether lowercase-hex 64-byte `actual` matches trusted `expected`.
/// Caller must supply the generated 64-byte expected capability; equal-format input
/// uses a complete XOR fold, though length/format rejection is deliberately early.
pub(super) fn equal_capability(
    actual: &str,
    expected: &str,
) -> bool {
    actual.len() == 64
        && actual.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && actual.bytes().zip(expected.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}
/// Returns JSON `value` with `status`, no-store/security headers and connection close.
/// Optional already-admitted `origin` adds CORS/Vary headers. A malformed header
/// value in `origin` panics; callers supply only validated fixed origin strings.
pub(super) fn response(
    status: StatusCode,
    value: Value,
    origin: Option<&str>,
) -> Response<Body> {
    let mut response = Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store")
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .header("content-security-policy", "default-src 'none'; frame-ancestors 'none'")
        .header("connection", "close")
        .body(Body::from(value.to_string()))
        .unwrap();
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert("access-control-allow-origin", HeaderValue::from_str(origin).unwrap());
        response
            .headers_mut()
            .insert("vary", HeaderValue::from_static("Origin"));
    }
    response
}
/// Returns the standard one-field JSON error response for `code` and `status`,
/// forwarding already-admitted `origin` to [`response`].
pub(super) fn error(
    status: StatusCode,
    code: &str,
    origin: Option<&str>,
) -> Response<Body> {
    response(status, json!({"error":code}), origin)
}
/// Returns the HTTP error response for job-store `e`: conflicts map to 409,
/// expiry to 410, unavailable/not-found to 404, others to 400. `origin` must be admitted.
pub(super) fn store_error(
    e: Error,
    origin: Option<&str>,
) -> Response<Body> {
    let status = match e {
        Error::Busy | Error::Closed | Error::InvalidState => StatusCode::CONFLICT,
        Error::Expired => StatusCode::GONE,
        Error::NotFoundOrUnavailable => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    error(status, &e.to_string(), origin)
}
/// Returns a validated polling request from exact decimal `after` and `limit`
/// parameters in `query`. Missing/duplicate/unknown keys, nondecimal/overflow values
/// and malformed pairs return schema errors; protocol range failures propagate.
pub(super) fn poll_query(query: Option<&str>) -> Result<PollRequest, Error> {
    let mut after = None;
    let mut limit = None;
    for pair in query.ok_or(Error::Schema)?.split('&') {
        let (k, v) = pair.split_once('=').ok_or(Error::Schema)?;
        if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Schema);
        }
        match k {
            "after" if after.is_none() => after = Some(v.parse().map_err(|_| Error::Schema)?),
            "limit" if limit.is_none() => limit = Some(v.parse().map_err(|_| Error::Schema)?),
            _ => return Err(Error::Schema),
        }
    }
    let p = PollRequest {
        after: after.ok_or(Error::Schema)?,
        limit: limit.ok_or(Error::Schema)?,
    };
    p.validate()?;
    Ok(p)
}
/// Returns at most `maximum` bytes from `body` within a ten-second Tokio deadline.
/// Non-data frames, read failures, overflow or timeout return `Err(())`; no partial
/// bytes escape. Cancellation drops the owned body without retaining partial input.
pub(super) async fn bounded_body(
    mut body: Body,
    maximum: usize,
) -> Result<Vec<u8>, ()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        while let Some(frame) = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await {
            let frame = frame.map_err(|_| ())?;
            let data = frame.data_ref().ok_or(())?;
            if data.len() > maximum.saturating_sub(bytes.len()) {
                return Err(());
            }
            bytes.extend_from_slice(data);
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| ())?
}
/// Returns true only if `body` ends within ten seconds with exclusively empty data
/// frames. Payload/trailers/read failure/timeout return false; cancellation drops it.
pub(super) async fn empty_body(mut body: Body) -> bool {
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(frame) = std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await {
            match frame {
                Ok(f) if f.data_ref().is_some_and(|b| b.is_empty()) => {}
                _ => return false,
            }
        }
        true
    })
    .await
    .unwrap_or(false)
}

/// Shared JSON framing for authenticated routes. Schema decoding stays with each
/// operation, and diagnostics retain their smaller independent request budget.
/// Returns `request` body bytes when the single content type is exactly
/// `application/json` and streaming bytes fit `maximum`. Wrong media type returns
/// 415/ExpectedJson; declared size, stream failure and timeout return 413/`bound_code`.
/// Content-Length is only an early hint; the stream limit remains authoritative.
pub(super) async fn json_body(
    request: Request<Body>,
    maximum: usize,
    bound_code: &'static str,
) -> Result<Vec<u8>, (StatusCode, &'static str)> {
    if one(request.headers(), "content-type") != Some("application/json") {
        return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "ExpectedJson"));
    }
    if one(request.headers(), "content-length").is_some_and(|v| v.parse::<usize>().map_or(true, |n| n > maximum)) {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, bound_code));
    }
    bounded_body(request.into_body(), maximum)
        .await
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, bound_code))
}
