import { useState, useRef, useEffect } from "react";
import "./LogPanel.css";

interface LogEntry {
  time: string;
  message: string;
  type: "info" | "error" | "success" | "warn";
}

const initialLogs: LogEntry[] = [
  { time: now(), message: "NarrativeStructure 已就绪", type: "success" },
  { time: now(), message: "等待指令...", type: "info" },
];

export default function LogPanel() {
  const [logs] = useState<LogEntry[]>(initialLogs);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  return (
    <div className="log-panel">
      <div className="log-header">
        <span>📋 处理日志</span>
        <span className="log-count">{logs.length} 条</span>
      </div>
      <div className="log-body">
        {logs.map((entry, i) => (
          <div key={i} className={`log-entry ${entry.type}`}>
            <span className="log-time">{entry.time}</span>
            <span className="log-msg">{entry.message}</span>
          </div>
        ))}
        <div ref={endRef} />
      </div>
    </div>
  );
}

function now(): string {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}
