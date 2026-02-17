use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::State,
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::info;

use crate::{MetricsCollector, MetricsSnapshot};

/// HTTP server for serving metrics
pub struct MetricsServer {
    collector: Arc<MetricsCollector>,
    port: u16,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(collector: Arc<MetricsCollector>, port: u16) -> Self {
        Self { collector, port }
    }

    /// Start the HTTP server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/", get(root_handler))
            .route("/metrics", get(metrics_handler))
            .route("/api/snapshot", get(snapshot_handler))
            .route("/health", get(health_handler))
            .with_state(self.collector.clone());

        // Bind only to localhost for security
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));

        info!("Starting metrics server on http://{}", addr);

        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Root handler - simple HTML landing page
async fn root_handler(State(collector): State<Arc<MetricsCollector>>) -> Html<String> {
    let snapshot = collector.get_snapshot();
    let uptime = collector.get_uptime_seconds();

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>WinEventEngine Metrics</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        h1 {{
            color: #333;
            border-bottom: 2px solid #4CAF50;
            padding-bottom: 10px;
        }}
        .metric-card {{
            background: white;
            border-radius: 8px;
            padding: 15px;
            margin: 10px 0;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .metric-value {{
            font-size: 2em;
            color: #4CAF50;
            font-weight: bold;
        }}
        .metric-label {{
            color: #666;
            font-size: 0.9em;
        }}
        .grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 15px;
        }}
        .endpoint {{
            background: #e3f2fd;
            padding: 10px;
            border-radius: 4px;
            font-family: monospace;
            margin: 5px 0;
        }}
        .uptime {{
            color: #2196F3;
            font-size: 1.2em;
        }}
    </style>
</head>
<body>
    <h1>Windows Event Engine Metrics</h1>
    
    <div class="metric-card">
        <div class="metric-label">Engine Uptime</div>
        <div class="uptime">{:.2} seconds</div>
    </div>

    <h2>Available Endpoints</h2>
    <div class="endpoint">/metrics - Prometheus format metrics</div>
    <div class="endpoint">/api/snapshot - JSON snapshot of all metrics</div>
    <div class="endpoint">/health - Health check</div>

    <h2>Quick Stats</h2>
    <div class="grid">
        <div class="metric-card">
            <div class="metric-label">Total Counters</div>
            <div class="metric-value">{}</div>
        </div>
        <div class="metric-card">
            <div class="metric-label">Total Gauges</div>
            <div class="metric-value">{}</div>
        </div>
        <div class="metric-card">
            <div class="metric-label">Total Histograms</div>
            <div class="metric-value">{}</div>
        </div>
    </div>

    <p><small>Metrics retention: 1 hour (24 hours for errors)</small></p>
</body>
</html>"#,
        uptime,
        snapshot.counters.len(),
        snapshot.gauges.len(),
        snapshot.histograms.len()
    );

    Html(html)
}

/// Prometheus format metrics handler
async fn metrics_handler(State(collector): State<Arc<MetricsCollector>>) -> String {
    collector.get_prometheus_format()
}

/// JSON snapshot handler
async fn snapshot_handler(State(collector): State<Arc<MetricsCollector>>) -> Json<MetricsSnapshot> {
    Json(collector.get_snapshot())
}

/// Health check handler
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_server_creation() {
        let collector = Arc::new(MetricsCollector::new());
        let server = MetricsServer::new(collector, 9090);
        
        // Server should be created successfully
        assert_eq!(server.port, 9090);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = health_handler().await;
        assert_eq!(response.status, "healthy");
    }
}
