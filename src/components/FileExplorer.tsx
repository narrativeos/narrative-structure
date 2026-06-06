import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./FileExplorer.css";

interface FileExplorerProps {
  projectPath: string;
}

const IMG_EXTS = /\.(png|jpg|jpeg|gif|svg|webp|bmp)$/i;

export default function FileExplorer({ projectPath }: FileExplorerProps) {
  const [files, setFiles] = useState<string[]>([]);

  useEffect(() => {
    if (!projectPath) return;
    invoke<string[]>("list_project_files")
      .then(setFiles)
      .catch(() => setFiles([]));
  }, [projectPath]);

  // 按父目录分组（取倒数第二个路径段作为目录名）
  const grouped: Record<string, string[]> = {};
  for (const f of files) {
    const parts = f.split(/[/\\]/);
    const dir = parts.length >= 2
      ? parts[parts.length - 2] + "/"
      : "";
    (grouped[dir] ??= []).push(f);
  }

  const extIcon = (name: string): string => {
    if (name.endsWith(".md")) return "📝";
    if (name.endsWith(".pdf")) return "📄";
    if (name.endsWith(".json")) return "📋";
    if (name.endsWith(".yaml")) return "⚙️";
    if (name.endsWith(".py")) return "🐍";
    return "📎";
  };

  const isImage = (name: string) => IMG_EXTS.test(name);

  const fileName = (path: string) => path.split(/[/\\]/).pop() || path;

  const fileUrl = (path: string) =>
    `narrativestructure://localhost/${encodeURIComponent(path)}?raw=1`;

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
              {isImage(f) ? (
                <img
                  src={fileUrl(f)}
                  alt=""
                  className="fe-thumb"
                  loading="lazy"
                />
              ) : (
                <span className="fe-icon">{extIcon(f)}</span>
              )}
              <span className="fe-name">{fileName(f)}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
