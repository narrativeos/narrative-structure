import { ReactNode, forwardRef } from "react";
import "./Workspace.css";

interface WorkspaceProps {
  children: ReactNode;
}

const Workspace = forwardRef<HTMLDivElement, WorkspaceProps>(
  function Workspace({ children }, ref) {
    return (
      <div className="workspace" ref={ref}>
        {children}
      </div>
    );
  }
);

export default Workspace;
