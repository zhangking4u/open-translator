use std::time::Duration;

use serde::{Deserialize, Serialize};

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败：{error}"))
}

#[derive(Serialize)]
struct TranslateRequest<'a> {
    text: &'a str,
    source: &'a str,
    target: &'a str,
}

#[derive(Deserialize)]
struct TranslateResponse {
    translation: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    kind: String,
    message: String,
}

pub async fn translate(
    client: &reqwest::Client,
    service_url: &str,
    source: &str,
    target: &str,
    text: &str,
) -> Result<String, String> {
    let url = format!("{}/translate", service_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&TranslateRequest {
            text,
            source,
            target,
        })
        .send()
        .await
        .map_err(|error| format!("无法连接翻译服务 {url}：{error}"))?;

    let status = response.status();

    if status.is_success() {
        let payload: TranslateResponse = response
            .json()
            .await
            .map_err(|error| format!("翻译服务返回了无效响应：{error}"))?;
        return Ok(payload.translation);
    }

    let body = response.text().await.unwrap_or_default();

    Err(service_error(status, &body))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    Delta {
        delta: String,
    },
    Done {
        translation: String,
    },
    Error {
        kind: String,
        message: String,
    },
}

/// Like [`translate`], but streams the translation through `POST
/// /translate/stream` and reports every delta through `on_delta`. Returns the
/// complete translation from the terminal event.
pub async fn translate_stream<F>(
    client: &reqwest::Client,
    service_url: &str,
    source: &str,
    target: &str,
    text: &str,
    mut on_delta: F,
) -> Result<String, String>
where
    F: FnMut(&str),
{
    let url = format!("{}/translate/stream", service_url.trim_end_matches('/'));

    let mut response = client
        .post(&url)
        .json(&TranslateRequest {
            text,
            source,
            target,
        })
        .send()
        .await
        .map_err(|error| format!("无法连接翻译服务 {url}：{error}"))?;

    let status = response.status();

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(service_error(status, &body));
    }

    let mut buffer = String::new();
    let mut translation = String::new();

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取流式响应失败：{error}"))?
    {
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(index) = buffer.find("\n\n") {
            let event: String = buffer.drain(..index + 2).collect();

            for line in event.lines() {
                let Some(event) = parse_event(line) else {
                    continue;
                };

                match event? {
                    StreamEvent::Delta { delta } => {
                        translation.push_str(&delta);
                        on_delta(&delta);
                    }
                    StreamEvent::Done {
                        translation: full,
                    } => return Ok(full),
                    StreamEvent::Error { kind, message } => {
                        return Err(format!("{}：{}", kind_label(&kind), message));
                    }
                }
            }
        }
    }

    Err("翻译服务提前关闭了流式响应".to_string())
}

fn parse_event(line: &str) -> Option<Result<StreamEvent, String>> {
    let data = line.strip_prefix("data:")?.trim();

    if data.is_empty() {
        return None;
    }

    Some(
        serde_json::from_str::<StreamEvent>(data)
            .map_err(|error| format!("解析流式响应失败：{error}")),
    )
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "invalid_request" => "请求无效",
        "engine_unavailable" => "翻译引擎不可用",
        "timeout" => "翻译超时",
        "internal" => "服务内部错误",
        other => other,
    }
}

fn service_error(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(error) = serde_json::from_str::<ErrorResponse>(body) {
        return format!("{}：{}", kind_label(&error.error.kind), error.error.message);
    }

    format!("翻译服务返回 {status}：{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delta_events() {
        let event = parse_event(r#"data: {"type":"delta","delta":"你好"}"#)
            .unwrap()
            .unwrap();

        match event {
            StreamEvent::Delta { delta } => assert_eq!(delta, "你好"),
            _ => panic!("expected a delta event"),
        }
    }

    #[test]
    fn ignores_comments_and_empty_lines() {
        assert!(parse_event(": keep-alive").is_none());
        assert!(parse_event("data:").is_none());
    }

    #[test]
    fn reports_malformed_events() {
        let error = parse_event("data: not-json").unwrap().unwrap_err();

        assert!(error.contains("解析流式响应失败"));
    }

    #[test]
    fn maps_error_kinds_to_chinese() {
        assert_eq!(kind_label("timeout"), "翻译超时");
        assert_eq!(kind_label("engine_unavailable"), "翻译引擎不可用");
        assert_eq!(kind_label("custom"), "custom");
    }

    #[tokio::test]
    async fn streams_events_from_a_stub_service() {
        use axum::body::Body;
        use axum::http::{StatusCode, header};
        use axum::response::Response;
        use axum::routing::post;
        use axum::Router;

        async fn stream() -> Response {
            let body = "data: {\"type\":\"delta\",\"delta\":\"内核\"}\n\n\
                        data: {\"type\":\"delta\",\"delta\":\"崩溃\"}\n\n\
                        data: {\"type\":\"done\",\"translation\":\"内核崩溃\",\"elapsed_ms\":12}\n\n";

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from(body))
                .unwrap()
        }

        let app = Router::new().route("/translate/stream", post(stream));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = build_client().unwrap();
        let mut deltas = Vec::new();

        let translation = translate_stream(
            &client,
            &format!("http://{addr}"),
            "en",
            "zh",
            "hello",
            |delta| deltas.push(delta.to_string()),
        )
        .await
        .unwrap();

        assert_eq!(translation, "内核崩溃");
        assert_eq!(deltas, vec!["内核".to_string(), "崩溃".to_string()]);
    }

    #[tokio::test]
    async fn surfaces_stream_error_events() {
        use axum::body::Body;
        use axum::http::{StatusCode, header};
        use axum::response::Response;
        use axum::routing::post;
        use axum::Router;

        async fn stream() -> Response {
            let body = "data: {\"type\":\"error\",\"kind\":\"timeout\",\"message\":\"translation timed out\"}\n\n";

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .body(Body::from(body))
                .unwrap()
        }

        let app = Router::new().route("/translate/stream", post(stream));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = build_client().unwrap();

        let error = translate_stream(
            &client,
            &format!("http://{addr}"),
            "en",
            "zh",
            "hello",
            |_| {},
        )
        .await
        .unwrap_err();

        assert_eq!(error, "翻译超时：translation timed out");
    }
}
