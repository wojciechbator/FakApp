//! The read-only board. Server-rendered HTML, one small stylesheet, zero
//! JavaScript: a watchdog dashboard must render on the worst phone on hotel
//! Wi-Fi while the thing it watches is on fire. No client script also keeps
//! Lighthouse at 100 across the board.

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::state::{MonitorState, Status};

pub struct WebState {
    pub shared: crate::Shared,
    pub config: crate::Config,
}

pub fn router(shared: crate::Shared, config: crate::Config) -> Router {
    let state = Arc::new(WebState { shared, config });
    Router::new()
        .route("/", get(dashboard))
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/llms.txt",
            get(|| async {
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; charset=utf-8",
                    )],
                    include_str!("../assets/llms.txt"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
                    ],
                    include_str!("../assets/style.css"),
                )
            }),
        )
        .route("/logo.svg", get(logo))
        .route("/favicon.svg", get(favicon))
        .with_state(state)
}

async fn dashboard(State(web): State<Arc<WebState>>) -> Response {
    let snapshot = web.shared.lock().await.clone();
    Html(render(&snapshot, &web.config)).into_response()
}

fn svg_asset(bytes: &'static str) -> Response {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn logo() -> Response {
    svg_asset(include_str!("../assets/logo.svg"))
}

async fn favicon() -> Response {
    svg_asset(include_str!("../assets/favicon.svg"))
}

struct TargetView {
    name: String,
    host: String,
    status: &'static str,
    healthy: bool,
    uptime_percent: Option<f64>,
    latency_ms: Option<u64>,
    checked_ago_secs: Option<u64>,
    since_change_secs: Option<u64>,
    last_error: Option<String>,
    recent: Vec<bool>,
}

fn escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            other => output.push(other),
        }
    }
    output
}

fn secs_ago(at: Option<std::time::SystemTime>) -> Option<u64> {
    let stamp = at?;
    let elapsed = std::time::SystemTime::now().duration_since(stamp).ok()?;
    Some(elapsed.as_secs())
}

fn human_ago(secs: Option<u64>) -> String {
    match secs {
        None => "never".to_owned(),
        Some(0) => "just now".to_owned(),
        Some(secs) if secs < 60 => format!("{secs}s ago"),
        Some(secs) if secs < 3600 => format!("{}m ago", secs / 60),
        Some(secs) => format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60),
    }
}

fn human_duration(secs: Option<u64>) -> String {
    match secs {
        None => "&#8212;".to_owned(),
        Some(secs) if secs < 60 => format!("{secs}s"),
        Some(secs) if secs < 3600 => format!("{}m {}s", secs / 60, secs % 60),
        Some(secs) => format!("{}h {}m", secs / 3600, (secs % 3600) / 60),
    }
}

fn project(shared: &MonitorState, config: &crate::Config) -> Vec<TargetView> {
    config
        .targets
        .iter()
        .map(|target| {
            let entry = shared.targets.get(&target.id).cloned().unwrap_or_default();
            let status = entry.status.unwrap_or(Status::Unknown);
            // History is oldest-first; the strip renders left-to-right.
            let recent: Vec<bool> = entry
                .history
                .iter()
                .rev()
                .take(40)
                .rev()
                .map(|record| record.ok)
                .collect();
            TargetView {
                name: target.name.clone(),
                host: url::Url::parse(&target.url)
                    .ok()
                    .and_then(|parsed| parsed.host_str().map(str::to_owned))
                    .unwrap_or_else(|| target.url.clone()),
                status: status.label(),
                healthy: status.healthy(),
                uptime_percent: {
                    let total = entry.history.len();
                    if total == 0 {
                        None
                    } else {
                        let ok = entry.history.iter().filter(|r| r.ok).count();
                        Some(((ok as f64 / total as f64) * 1000.0).round() / 10.0)
                    }
                },
                latency_ms: entry
                    .history
                    .iter()
                    .rev()
                    .find(|record| record.ok)
                    .and_then(|record| record.latency_ms),
                checked_ago_secs: secs_ago(entry.history.back().map(|r| r.at)),
                since_change_secs: secs_ago(entry.last_change),
                last_error: entry
                    .history
                    .iter()
                    .rev()
                    .find(|record| !record.ok)
                    .and_then(|record| record.error.clone()),
                recent,
            }
        })
        .collect()
}

fn render(shared: &MonitorState, config: &crate::Config) -> String {
    let targets = project(shared, config);
    let down_count = targets.iter().filter(|t| !t.healthy).count();

    let banner = if targets.is_empty() {
        ("NO TARGETS CONFIGURED", "banner-warn")
    } else if down_count == 0 {
        ("ALL SYSTEMS GO", "banner-ok")
    } else {
        (
            if down_count == 1 {
                "1 SERVICE DOWN"
            } else {
                "SERVICES DOWN"
            },
            "banner-bad",
        )
    };

    let cards = targets
        .iter()
        .map(|target| {
            let strip: String = target
                .recent
                .iter()
                .map(|ok| {
                    if *ok {
                        "<i class=\"bar bar-ok\"></i>"
                    } else {
                        "<i class=\"bar bar-fail\"></i>"
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            let error_line = target
                .last_error
                .as_deref()
                .map(|error| {
                    format!(
                        "<p class=\"target-error\">last failure: {}</p>",
                        escape(error)
                    )
                })
                .unwrap_or_default();
            let latency = target
                .latency_ms
                .map(|ms| format!("{ms} ms"))
                .unwrap_or_else(|| "&#8212;".to_owned());
            let uptime = target
                .uptime_percent
                .map(|percent| format!("{percent:.1}%"))
                .unwrap_or_else(|| "&#8212;".to_owned());
            format!(
                concat!(
                    "<article class=\"panel target-panel\">",
                    "<header class=\"target-head\">",
                    "<div><span class=\"eyebrow\">{host}</span><h2>{name}</h2></div>",
                    "<span class=\"badge badge-{tone}\">{status}</span>",
                    "</header>",
                    "<dl class=\"target-facts\">",
                    "<div><dt>Latency</dt><dd>{latency}</dd></div>",
                    "<div><dt>Uptime</dt><dd>{uptime}</dd></div>",
                    "<div><dt>In state</dt><dd>{in_state}</dd></div>",
                    "<div><dt>Last check</dt><dd>{checked}</dd></div>",
                    "</dl>",
                    "<p class=\"strip\" aria-label=\"recent checks, oldest to newest\">{strip}</p>",
                    "{error_line}",
                    "</article>"
                ),
                host = escape(&target.host),
                name = escape(&target.name),
                tone = if target.healthy { "good" } else { "bad" },
                status = target.status,
                latency = latency,
                uptime = uptime,
                in_state = human_duration(target.since_change_secs),
                checked = escape(&human_ago(target.checked_ago_secs)),
                strip = strip,
                error_line = error_line,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        include_str!("../assets/board.html"),
        banner_text = banner.0,
        banner_class = banner.1,
        generated = rfc3339_now(),
        cards = cards,
    )
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::from(std::time::SystemTime::now())
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
