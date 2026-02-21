# Web Dashboard

Real-time monitoring dashboard for the Windows Event Automation Engine.

## Overview

The web dashboard provides live visualization of engine activity through a web interface. It connects via WebSocket to receive real-time updates as events are processed.

![Dashboard Overview](images/dashboard-overview.png)
*Main dashboard showing live metrics and event stream*

## Accessing the Dashboard

Once the engine is running, open your browser:

```
http://127.0.0.1:9090
```

**Security Note**: The dashboard is only accessible from localhost (`127.0.0.1`) for security. It cannot be accessed from other machines on the network.

## Dashboard Features

### 1. Real-Time Event Stream

![Event Stream](images/event-stream.png)
*Live event feed showing processed events*

The center panel displays events as they occur:
- **Event Type**: FileCreated, ProcessStarted, etc.
- **Source**: Which plugin generated the event
- **Timestamp**: When the event occurred
- **Details**: Event-specific metadata (path, process name, etc.)

**Filter Events**: Use the buttons at the top to filter:
- **All**: Show all events
- **Events**: File system, window, process events
- **Rules**: Rule evaluations and matches
- **Actions**: Action executions

### 2. Live Metrics Charts

![Metrics Charts](images/metrics-charts.png)
*Real-time charts showing system activity*

Four key metrics are displayed:

#### Events/sec
- Shows the rate of events being processed
- 60-second rolling history
- Helps identify spikes in activity

#### Rule Matches/sec
- Shows how many rules are matching per second
- Indicates how busy your automation is

#### Actions Executed
- Bar chart showing successful vs failed actions
- Green = Success, Red = Error

#### System Health
- **Uptime**: How long the engine has been running
- **Active Plugins**: Number of running event sources
- **Active Rules**: Number of enabled rules

### 3. Connection Status

![Connection Status](images/connection-status.png)
*WebSocket connection indicator*

Top-right corner shows connection state:
- 🟢 **Green**: Connected and receiving updates
- 🔴 **Red**: Disconnected (will auto-reconnect)

## How It Works

### WebSocket Connection

The dashboard establishes a WebSocket connection to the engine:

```
Browser ←→ WebSocket (ws://127.0.0.1:9090/ws) ←→ Engine
```

**Connection Features**:
- ✅ Auto-reconnect if connection drops
- ✅ Exponential backoff for reconnection attempts
- ✅ Sends full metrics snapshot every 5 seconds
- ✅ Pushes events immediately as they happen

### Data Flow

1. **Event occurs** (file created, process started, etc.)
2. **Engine processes** the event through rules
3. **Metrics updated** and broadcast via WebSocket
4. **Dashboard receives** update and refreshes display

**Latency**: Typically <100ms from event to dashboard display.

## Use Cases

### 1. Monitoring File Processing

Watch files being processed in real-time:
```
[Event] FileCreated: C:/Data/input.csv
[Rule] Data processing rule matched
[Action] Execute: python process.py
[Event] FileCreated: C:/Data/output.json
```

### 2. Debugging Rule Logic

See which rules are matching and why:
```
[Rule] backup_important_files evaluated
[Rule] backup_important_files MATCHED
[Action] Script: backup.lua executed successfully
```

### 3. Performance Monitoring

Identify bottlenecks:
- High events/sec but low rule matches? → Rules may be too restrictive
- High action failures? → Check action configurations
- Low events/sec? → Verify sources are configured correctly

### 4. System Health Check

At a glance:
- Is the engine running? ✅ Uptime counter
- Are plugins active? ✅ Active plugins count
- Are rules working? ✅ Rule match rate

## API Endpoints

The dashboard server also provides these endpoints:

### Prometheus Metrics
```
GET http://127.0.0.1:9090/metrics
```
Returns metrics in Prometheus format for integration with monitoring systems.

### JSON Snapshot
```
GET http://127.0.0.1:9090/api/snapshot
```
Returns current metrics as JSON:
```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "counters": {
    "events_total": 150,
    "rules_matched_total": 45
  },
  "gauges": {
    "engine_uptime_seconds": 3600
  }
}
```

### Health Check
```
GET http://127.0.0.1:9090/health
```
Returns health status:
```json
{
  "status": "healthy",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

## Troubleshooting

### Dashboard Won't Load

**Problem**: Browser shows "This site can't be reached"

**Solutions**:
1. Verify engine is running: Check console output
2. Check port availability: `netstat -an | findstr 9090`
3. Try refreshing the page
4. Check firewall isn't blocking localhost

### No Events Showing

**Problem**: Dashboard loads but event stream is empty

**Solutions**:
1. Verify event sources are configured and enabled
2. Check that rules match your test events
3. Look at engine logs for errors
4. Try triggering a test event manually

### Connection Drops Frequently

**Problem**: Red dot keeps appearing

**Solutions**:
1. Check for high CPU/memory usage
2. Verify network stability (though it's localhost)
3. Check engine logs for errors
4. May need to restart the engine

### Charts Not Updating

**Problem**: Metrics stuck at 0

**Solutions**:
1. Check if events are being processed (check event stream)
2. Verify WebSocket connection (green dot)
3. Refresh the page
4. Check browser console for JavaScript errors

## Customization

### Changing Port

Edit the engine configuration or modify the source to use a different port:

```rust
// In main.rs
let metrics_server = MetricsServer::new(metrics, 9090); // Change 9090
```

### Adding Custom Panels

The dashboard is a single HTML file. You can customize it by editing:
```
metrics/src/server.rs (DASHBOARD_HTML constant)
```

### Styling

The dashboard uses embedded CSS. Modify the `<style>` section in `server.rs` to change:
- Colors
- Layout
- Font sizes
- Chart appearance

## Performance

### Resource Usage

- **CPU**: Minimal (<1% on modern hardware)
- **Memory**: ~10-20MB for dashboard server
- **Network**: Localhost only, minimal bandwidth
- **Browser**: Works on any modern browser

### Scalability

The dashboard handles:
- ✅ 1000+ events per second
- ✅ Multiple browser tabs
- ✅ Automatic cleanup of old events (keeps last 100)

## Security

### Localhost Only

The server binds exclusively to `127.0.0.1`:
```rust
let addr = SocketAddr::from(([127, 0, 0, 1], 9090));
```

This means:
- ✅ Cannot be accessed from other machines
- ✅ Cannot be accessed via external IP
- ✅ Safe to run on public-facing servers

### No Authentication

Currently no authentication is required because it's localhost-only. If you need to expose the dashboard:

1. Use a reverse proxy (nginx, Apache)
2. Add authentication at the proxy level
3. Consider using HTTPS

## See Also

- [Configuration Reference](Configuration-Reference) - Configuring metrics endpoint
- [Architecture](Architecture) - How the dashboard fits into the system
- [Troubleshooting](Troubleshooting) - General troubleshooting guide
