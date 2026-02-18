use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, Json},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::time::{interval, Duration};
use tracing::{error, info};

use crate::{MetricUpdate, MetricsCollector, MetricsSnapshot};

/// HTTP server for serving metrics with WebSocket support
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
            .route("/ws", get(websocket_handler))
            .with_state(self.collector.clone());

        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        info!("Starting metrics server on http://{}", addr);
        info!("WebSocket endpoint available at ws://{}/ws", addr);

        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// WebSocket handler - upgrades HTTP to WebSocket connection
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(collector): State<Arc<MetricsCollector>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, collector))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, collector: Arc<MetricsCollector>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to metric updates
    let mut updates = collector.subscribe();

    // Send initial snapshot
    let initial_snapshot = MetricUpdate::Snapshot(collector.get_snapshot());
    if let Ok(json) = serde_json::to_string(&initial_snapshot) {
        let _ = sender.send(Message::Text(json)).await;
    }

    // Create periodic snapshot interval (every 5 seconds)
    let mut snapshot_interval = interval(Duration::from_secs(5));

    info!("New WebSocket client connected");

    loop {
        tokio::select! {
            // Receive broadcast updates from metrics collector
            Ok(update) = updates.recv() => {
                match serde_json::to_string(&update) {
                    Ok(json) => {
                        if sender.send(Message::Text(json)).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize metric update: {}", e);
                    }
                }
            }

            // Send periodic snapshots
            _ = snapshot_interval.tick() => {
                let snapshot = MetricUpdate::Snapshot(collector.get_snapshot());
                if let Ok(json) = serde_json::to_string(&snapshot) {
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }

            // Handle client messages (ping/pong, commands)
            Some(Ok(msg)) = receiver.next() => {
                match msg {
                    Message::Close(_) => {
                        info!("WebSocket client disconnected");
                        break;
                    }
                    Message::Ping(data) => {
                        if sender.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => {
                        handle_client_command(&text, &collector).await;
                    }
                    _ => {}
                }
            }

            else => break,
        }
    }
}

/// Handle optional client commands via WebSocket
async fn handle_client_command(text: &str, _collector: &MetricsCollector) {
    if let Ok(cmd) = serde_json::from_str::<ClientCommand>(text) {
        match cmd {
            ClientCommand::Ping => {
                // Pong sent automatically by protocol
            }
        }
    }
}

#[derive(Debug, Deserialize)]
enum ClientCommand {
    Ping,
}

/// Root handler - full dashboard HTML
async fn root_handler(State(_collector): State<Arc<MetricsCollector>>) -> Html<String> {
    Html(DASHBOARD_HTML.to_string())
}

/// Full dashboard HTML with embedded CSS and JavaScript
const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>WinEventEngine Dashboard</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
    <style>
        * {
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0f172a;
            color: #e2e8f0;
            min-height: 100vh;
        }

        .header {
            background: linear-gradient(135deg, #1e293b 0%, #0f172a 100%);
            padding: 1.5rem 2rem;
            border-bottom: 1px solid #334155;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .header h1 {
            font-size: 1.5rem;
            color: #f8fafc;
        }

        .connection-status {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            font-size: 0.875rem;
        }

        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: #ef4444;
            transition: background 0.3s;
        }

        .status-dot.connected {
            background: #22c55e;
        }

        .container {
            max-width: 1400px;
            margin: 0 auto;
            padding: 2rem;
        }

        .grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 1.5rem;
            margin-bottom: 2rem;
        }

        .card {
            background: #1e293b;
            border-radius: 12px;
            padding: 1.5rem;
            border: 1px solid #334155;
        }

        .card h3 {
            font-size: 0.875rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            color: #94a3b8;
            margin-bottom: 1rem;
        }

        .metric-value {
            font-size: 2.5rem;
            font-weight: 700;
            color: #f8fafc;
        }

        .metric-value.success { color: #22c55e; }
        .metric-value.warning { color: #f59e0b; }
        .metric-value.error { color: #ef4444; }

        .chart-container {
            position: relative;
            height: 200px;
            margin-top: 1rem;
        }

        .event-log {
            max-height: 400px;
            overflow-y: auto;
        }

        .event-item {
            padding: 0.75rem;
            border-bottom: 1px solid #334155;
            font-size: 0.875rem;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .event-item:last-child {
            border-bottom: none;
        }

        .event-time {
            color: #64748b;
            font-size: 0.75rem;
        }

        .event-type {
            background: #334155;
            padding: 0.25rem 0.5rem;
            border-radius: 4px;
            font-size: 0.75rem;
        }

        .filters {
            display: flex;
            gap: 0.5rem;
            margin-bottom: 1rem;
        }

        .filter-btn {
            background: #334155;
            border: none;
            color: #e2e8f0;
            padding: 0.5rem 1rem;
            border-radius: 6px;
            cursor: pointer;
            font-size: 0.875rem;
            transition: background 0.2s;
        }

        .filter-btn:hover, .filter-btn.active {
            background: #475569;
        }

        @media (max-width: 768px) {
            .container {
                padding: 1rem;
            }
            .grid {
                grid-template-columns: 1fr;
            }
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>WinEventEngine Dashboard</h1>
        <div class="connection-status">
            <div class="status-dot" id="statusDot"></div>
            <span id="statusText">Connecting...</span>
        </div>
    </div>

    <div class="container">
        <!-- Top Metrics Row -->
        <div class="grid">
            <div class="card">
                <h3>Events/sec</h3>
                <div class="metric-value" id="eventsPerSec">0</div>
                <div class="chart-container">
                    <canvas id="eventsChart"></canvas>
                </div>
            </div>
            <div class="card">
                <h3>Rule Matches/sec</h3>
                <div class="metric-value" id="matchesPerSec">0</div>
                <div class="chart-container">
                    <canvas id="matchesChart"></canvas>
                </div>
            </div>
            <div class="card">
                <h3>Actions Executed</h3>
                <div class="metric-value success" id="actionsCount">0</div>
                <div class="chart-container">
                    <canvas id="actionsChart"></canvas>
                </div>
            </div>
            <div class="card">
                <h3>System Health</h3>
                <div class="metric-value" id="uptime">0s</div>
                <div style="margin-top: 1rem; font-size: 0.875rem; color: #94a3b8;">
                    <div>Plugins: <span id="pluginCount">0</span></div>
                    <div>Rules: <span id="ruleCount">0</span></div>
                </div>
            </div>
        </div>

        <!-- Event Log -->
        <div class="card">
            <h3>Live Event Stream</h3>
            <div class="filters">
                <button class="filter-btn active" onclick="filterEvents('all')">All</button>
                <button class="filter-btn" onclick="filterEvents('event')">Events</button>
                <button class="filter-btn" onclick="filterEvents('rule')">Rules</button>
                <button class="filter-btn" onclick="filterEvents('action')">Actions</button>
            </div>
            <div class="event-log" id="eventLog">
                <div style="text-align: center; color: #64748b; padding: 2rem;">
                    Waiting for events...
                </div>
            </div>
        </div>
    </div>

    <script>
        // WebSocket connection management
        let ws = null;
        let reconnectInterval = 1000;
        let maxReconnectInterval = 30000;
        let eventBuffer = [];
        let currentFilter = 'all';

        // Chart instances
        let eventsChart, matchesChart, actionsChart;

        // Data buffers for charts (keep last 60 seconds)
        const eventsData = new Array(60).fill(0);
        const matchesData = new Array(60).fill(0);
        const actionsData = new Array(60).fill(0);

        // Initialize charts
        function initCharts() {
            const chartOptions = {
                responsive: true,
                maintainAspectRatio: false,
                plugins: { legend: { display: false } },
                scales: {
                    x: { display: false },
                    y: {
                        beginAtZero: true,
                        grid: { color: '#334155' },
                        ticks: { color: '#94a3b8', font: { size: 10 } }
                    }
                },
                elements: {
                    line: { tension: 0.4 },
                    point: { radius: 0 }
                }
            };

            eventsChart = new Chart(document.getElementById('eventsChart'), {
                type: 'line',
                data: {
                    labels: new Array(60).fill(''),
                    datasets: [{
                        data: eventsData,
                        borderColor: '#3b82f6',
                        backgroundColor: 'rgba(59, 130, 246, 0.1)',
                        fill: true,
                        borderWidth: 2
                    }]
                },
                options: chartOptions
            });

            matchesChart = new Chart(document.getElementById('matchesChart'), {
                type: 'line',
                data: {
                    labels: new Array(60).fill(''),
                    datasets: [{
                        data: matchesData,
                        borderColor: '#22c55e',
                        backgroundColor: 'rgba(34, 197, 94, 0.1)',
                        fill: true,
                        borderWidth: 2
                    }]
                },
                options: chartOptions
            });

            actionsChart = new Chart(document.getElementById('actionsChart'), {
                type: 'bar',
                data: {
                    labels: ['Success', 'Error'],
                    datasets: [{
                        data: [0, 0],
                        backgroundColor: ['#22c55e', '#ef4444'],
                        borderWidth: 0
                    }]
                },
                options: {
                    ...chartOptions,
                    scales: {
                        y: { beginAtZero: true, grid: { display: false }, ticks: { display: false } },
                        x: { grid: { display: false }, ticks: { color: '#94a3b8', font: { size: 10 } } }
                    }
                }
            });
        }

        // Connect to WebSocket
        function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws`;

            ws = new WebSocket(wsUrl);

            ws.onopen = () => {
                console.log('WebSocket connected');
                updateStatus(true);
                reconnectInterval = 1000;
            };

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    handleMessage(data);
                } catch (e) {
                    console.error('Failed to parse message:', e);
                }
            };

            ws.onclose = () => {
                console.log('WebSocket disconnected');
                updateStatus(false);
                scheduleReconnect();
            };

            ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                ws.close();
            };
        }

        // Reconnection logic with exponential backoff
        function scheduleReconnect() {
            setTimeout(() => {
                console.log(`Reconnecting in ${reconnectInterval}ms...`);
                reconnectInterval = Math.min(reconnectInterval * 2, maxReconnectInterval);
                connect();
            }, reconnectInterval);
        }

        // Update connection status UI
        function updateStatus(connected) {
            const dot = document.getElementById('statusDot');
            const text = document.getElementById('statusText');

            if (connected) {
                dot.classList.add('connected');
                text.textContent = 'Connected';
            } else {
                dot.classList.remove('connected');
                text.textContent = 'Disconnected';
            }
        }

        // Handle incoming WebSocket messages
        let eventCount = 0;
        let matchCount = 0;
        let actionSuccessCount = 0;
        let actionErrorCount = 0;
        let lastSecondEvents = 0;
        let lastSecondMatches = 0;

        function handleMessage(data) {
            switch(data.type) {
                case 'event_received':
                    lastSecondEvents++;
                    addEventToLog('event', `Event from ${data.data.source}`, data.data.event_type);
                    break;

                case 'rule_matched':
                    lastSecondMatches++;
                    addEventToLog('rule', `Rule matched: ${data.data.rule_name}`, 'match');
                    break;

                case 'action_executed':
                    if (data.data.success) {
                        actionSuccessCount++;
                    } else {
                        actionErrorCount++;
                    }
                    addEventToLog('action', `Action: ${data.data.action_name}`,
                        data.data.success ? 'success' : 'error');
                    break;

                case 'snapshot':
                    updateSnapshot(data.data);
                    break;
            }
        }

        // Add event to live log
        function addEventToLog(type, message, detail) {
            const log = document.getElementById('eventLog');

            if (log.children.length === 1 && log.children[0].style.textAlign === 'center') {
                log.innerHTML = '';
            }

            const item = document.createElement('div');
            item.className = 'event-item';
            item.dataset.type = type;

            const time = new Date().toLocaleTimeString();
            item.innerHTML = `
                <div>
                    <span class="event-type">${type}</span>
                    ${message}
                </div>
                <div style="text-align: right;">
                    <div style="color: #94a3b8; font-size: 0.75rem;">${detail}</div>
                    <div class="event-time">${time}</div>
                </div>
            `;

            log.insertBefore(item, log.firstChild);

            while (log.children.length > 100) {
                log.removeChild(log.lastChild);
            }

            applyFilter();
        }

        // Filter events in the log
        function filterEvents(type) {
            currentFilter = type;

            document.querySelectorAll('.filter-btn').forEach(btn => {
                btn.classList.remove('active');
                if (btn.textContent.toLowerCase().includes(type) ||
                    (type === 'all' && btn.textContent === 'All')) {
                    btn.classList.add('active');
                }
            });

            applyFilter();
        }

        function applyFilter() {
            document.querySelectorAll('.event-item').forEach(item => {
                if (currentFilter === 'all' || item.dataset.type === currentFilter) {
                    item.style.display = 'flex';
                } else {
                    item.style.display = 'none';
                }
            });
        }

        // Update UI from snapshot
        function updateSnapshot(snapshot) {
            document.getElementById('pluginCount').textContent =
                snapshot.gauges && snapshot.gauges.active_plugins ? snapshot.gauges.active_plugins : 0;
            document.getElementById('ruleCount').textContent =
                snapshot.gauges && snapshot.gauges.active_rules ? snapshot.gauges.active_rules : 0;
        }

        // Update charts and metrics every second
        setInterval(() => {
            document.getElementById('eventsPerSec').textContent = lastSecondEvents;
            document.getElementById('matchesPerSec').textContent = lastSecondMatches;

            eventsData.shift();
            eventsData.push(lastSecondEvents);
            eventsChart.update('none');

            matchesData.shift();
            matchesData.push(lastSecondMatches);
            matchesChart.update('none');

            document.getElementById('actionsCount').textContent =
                actionSuccessCount + actionErrorCount;
            actionsChart.data.datasets[0].data = [actionSuccessCount, actionErrorCount];
            actionsChart.update('none');

            lastSecondEvents = 0;
            lastSecondMatches = 0;

            const uptime = document.getElementById('uptime');
            const current = parseInt(uptime.textContent) || 0;
            uptime.textContent = (current + 1) + 's';
        }, 1000);

        // Initialize
        document.addEventListener('DOMContentLoaded', () => {
            initCharts();
            connect();
        });

        // Cleanup on page unload
        window.addEventListener('beforeunload', () => {
            if (ws) {
                ws.close();
            }
        });
    </script>
</body>
</html>"#;

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

        assert_eq!(server.port, 9090);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = health_handler().await;
        assert_eq!(response.status, "healthy");
    }
}
