import { ReactNode } from "react";
import "./Workspace.css";

interface WorkspaceProps {
  children: ReactNode;
}

/** 工作区容器：TOC + PDF + Editor 的统一包裹，提供 position:relative 锚点 */
export default function Workspace({ children }: WorkspaceProps) {
  return (
    <div className="workspace">
      {children}
    </div>
  );
}
