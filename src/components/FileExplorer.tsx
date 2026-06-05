import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./FileExplorer.css";

interface FileExplorerProps {
  projectPath: string;
}

export default function FileExplorer({ projectPath }: FileExplorerProps) {
  const [files, setFiles] = useState<string[]>([]);

  useEffect(() => {
    if (!projectPath) return;
    invoke<string[]>("list_project_files")
      .then(setFiles)
      .catch(() => setFiles([]));
  }, [projectPath]);

  const grouped: Record<string, string[]> = {};
  for (const f of files) {
    const dir = f.includes("/") ? f.split("/")[0] + "/" : "";
    (grouped[dir] ??= []).push(f);
  }

  const extIcon = (name: string): string => {
    if (name.endsWith(".md")) return "📝";
    if (name.endsWith(".pdf")) return "📄";
    if (name.endsWith(".json")) return "📋";
    if (name.endsWith(".yaml")) return "⚙️";
    if (name.endsWith(".py")) return "🐍";
    if (/\.(png|jpg|jpeg|gif|svg)$/i.test(name)) return "🖼️";
    return "📎";
  };

  return (
    <div className="file-explorer">
      <h4 className="fe-title">📁 文件资产</h4>
      {Object.keys(grouped).length === 0 && (
        <p className="fe-empty">暂无文件</p>
      )}
      {Object.entries(grouped).map(([dir, items]) => (
        <div key={dir} className="fe-group">
          {dir && <div className="fe-dir">{dir}</div>}
          {items.map((f) => (
            <div key={f} className="fe-item" title={f}>
              <span className="fe-icon">{extIcon(f)}</span>
              <span className="fe-name">{dir ? f.replace(dir, "") : f}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
