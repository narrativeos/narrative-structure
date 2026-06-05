import { useState, useRef, useEffect } from "react";
import "./AgentConsole.css";

interface AgentConsoleProps {
  onExecute?: (command: string) => void;
}

interface LogEntry {
  time: string;
  message: string;
  type: "info" | "error" | "success";
}

export default function AgentConsole({ onExecute }: AgentConsoleProps) {
  const [command, setCommand] = useState("");
  const [logs, setLogs] = useState<LogEntry[]>([
    { time: now(), message: "欢迎使用 NarrativeStructure 智能控制台", type: "info" },
    { time: now(), message: "输入重构指令，如：'将所有二级标题转为三级'", type: "info" },
  ]);
  const [expanded, setExpanded] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const addLog = (message: string, type: LogEntry["type"] = "info") => {
    setLogs((prev) => [...prev, { time: now(), message, type }]);
  };

  const handleExecute = () => {
    const cmd = command.trim();
    if (!cmd) return;
    addLog(`> ${cmd}`, "info");
    setCommand("");
    onExecute?.(cmd);
  };

  return (
    <div className={`agent-console ${expanded ? "expanded" : ""}`}>
      <div className="ac-input-row">
        <input
          className="ac-input"
          type="text"
          placeholder="输入重构指令..."
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleExecute()}
        />
        <button className="ac-btn" onClick={handleExecute}>▶</button>
        <button className="ac-toggle" onClick={() => setExpanded(!expanded)}>
          {expanded ? "▼" : "▲"}
        </button>
      </div>
      {expanded && (
        <div className="ac-log">
          {logs.map((entry, i) => (
            <div key={i} className={`ac-log-entry ${entry.type}`}>
              <span className="ac-log-time">{entry.time}</span>
              <span className="ac-log-msg">{entry.message}</span>
            </div>
          ))}
          <div ref={logEndRef} />
        </div>
      )}
    </div>
  );
}

function now(): string {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}
