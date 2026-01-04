import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { LogEntry } from "../types";
import * as api from "../api";
import "./LogsViewer.css";

export function LogsViewer() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  const loadLogs = async () => {
    try {
      const data = await api.getLogs(undefined, 100);
      setLogs(data);
    } catch (err) {
      console.error("Failed to load logs:", err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    loadLogs();

    // Listen for real-time log updates
    const unlisten = listen<LogEntry>("log", (event) => {
      setLogs((prev) => [event.payload, ...prev].slice(0, 100));
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleClear = async () => {
    try {
      await api.clearLogs();
      setLogs([]);
    } catch (err) {
      console.error("Failed to clear logs:", err);
    }
  };

  const getLogIcon = (level: LogEntry["level"]) => {
    switch (level) {
      case "success":
        return "✓";
      case "error":
        return "✕";
      case "warning":
        return "!";
      default:
        return "•";
    }
  };

  const getSourceLabel = (source: LogEntry["source"]) => {
    switch (source) {
      case "schema":
        return "Schema";
      case "edge_function":
        return "Function";
      case "watcher":
        return "Watcher";
      case "system":
        return "System";
      default:
        return source;
    }
  };

  const formatTime = (timestamp: string) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
  };

  if (isLoading) {
    return <div className="loading">Loading logs...</div>;
  }

  return (
    <div className="logs-viewer">
      <div className="logs-header">
        <span className="logs-title">Activity Log</span>
        {logs.length > 0 && (
          <button className="clear-btn" onClick={handleClear}>
            Clear
          </button>
        )}
      </div>

      {logs.length === 0 ? (
        <div className="empty-state">
          <p>No logs yet</p>
          <p className="hint">Activity will appear here as you work</p>
        </div>
      ) : (
        <div className="logs-list">
          {logs.map((log) => (
            <div key={log.id} className={`log-entry ${log.level}`}>
              <span className="log-icon">{getLogIcon(log.level)}</span>
              <div className="log-content">
                <div className="log-header">
                  <span className="log-source">{getSourceLabel(log.source)}</span>
                  <span className="log-time">{formatTime(log.timestamp)}</span>
                </div>
                <div className="log-message">{log.message}</div>
                {log.details && (
                  <div className="log-details">{log.details}</div>
                )}
              </div>
            </div>
          ))}
          <div ref={logsEndRef} />
        </div>
      )}
    </div>
  );
}
