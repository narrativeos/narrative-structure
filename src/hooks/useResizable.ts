import { useCallback, useRef, useState } from "react";

/** 可拖拽手柄 hook */
export function useResizable(
  initial: number,
  min: number,
  max: number,
  axis: "x" | "y" = "x"
): [number, (opts?: ResizeOpts) => React.HTMLAttributes<HTMLDivElement>] {
  const [size, setSize] = useState(initial);
  const containerRef = useRef<number>(0);

  const bindHandle = useCallback(
    (opts?: ResizeOpts) => {
      const { reversed = false, usePercent = false, getContainerWidth } = opts || {};

      const onMouseDown = (e: React.MouseEvent) => {
        e.preventDefault();
        const startPos = axis === "x" ? e.clientX : e.clientY;
        const startSize = size;
        if (usePercent && getContainerWidth) {
          containerRef.current = getContainerWidth();
        }

        const move = (ev: MouseEvent) => {
          const currentPos = axis === "x" ? ev.clientX : ev.clientY;
          let delta = currentPos - startPos;
          if (reversed) delta = -delta;
          if (usePercent && containerRef.current > 0) {
            delta = (delta / containerRef.current) * 100;
          }
          const newSize = Math.min(max, Math.max(min, startSize + delta));
          setSize(newSize);
        };

        const up = () => {
          document.removeEventListener("mousemove", move);
          document.removeEventListener("mouseup", up);
        };
        document.addEventListener("mousemove", move);
        document.addEventListener("mouseup", up);
      };

      return { onMouseDown, role: "separator" as const, tabIndex: -1 };
    },
    [size, min, max, axis],
  );

  return [size, bindHandle];
}

interface ResizeOpts {
  reversed?: boolean;
  usePercent?: boolean;
  getContainerWidth?: () => number;
}

