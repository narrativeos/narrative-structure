import { useState } from "react";
import "./AgentConsole.css";

interface AgentConsoleProps {
  onExecute?: (command: string) => void;
}

export default function AgentConsole({ onExecute }: AgentConsoleProps) {
  const [command, setCommand] = useState("");

  const handleExecute = () => {
    const cmd = command.trim();
    if (!cmd) return;
    setCommand("");
    onExecute?.(cmd);
  };

  return (
    <div className="agent-console-compact">
      <textarea
        className="ac-textarea"
        rows={3}
        placeholder="输入重构指令...&#10;如：将二级标题统一为三级"
        value={command}
        onChange={(e) => setCommand(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleExecute();
          }
        }}
      />
      <button className="ac-btn" onClick={handleExecute}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><polygon points="5 3 19 12 5 21 5 3"/></svg>
        执行
      </button>
    </div>
  );
}
