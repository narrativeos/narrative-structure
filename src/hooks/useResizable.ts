import { useCallback, useRef, useState } from "react";

/** 可拖拽手柄 hook，返回 [当前尺寸, 手柄 props] */
export function useResizable(
  initial: number,
  min: number,
  max: number,
  axis: "x" | "y" = "x"
): [number, (isReversed?: boolean) => React.HTMLAttributes<HTMLDivElement>] {
  const [size, setSize] = useState(initial);
  const dragRef = useRef<{ start: number; startSize: number } | null>(null);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragRef.current = {
        start: axis === "x" ? e.clientX : e.clientY,
        startSize: size,
      };
    },
    [size, axis],
  );

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!dragRef.current) return;
      const delta = (axis === "x" ? e.clientX : e.clientY) - dragRef.current.start;
      const newSize = Math.min(max, Math.max(min, dragRef.current.startSize + delta));
      setSize(newSize);
    },
    [min, max, axis],
  );

  const handleMouseUp = useCallback(() => {
    dragRef.current = null;
  }, []);

  const bindHandle = useCallback(
    (isReversed = false) => {
      const onMouseDown = (e: React.MouseEvent) => {
        // reversed: delta 方向相反
        if (isReversed) {
          e.preventDefault();
          dragRef.current = {
            start: axis === "x" ? e.clientX : e.clientY,
            startSize: size,
          };
          const move = (ev: MouseEvent) => {
            if (!dragRef.current) return;
            const delta = (axis === "x" ? ev.clientX : ev.clientY) - dragRef.current.start;
            const newSize = Math.min(max, Math.max(min, dragRef.current.startSize - delta));
            setSize(newSize);
          };
          const up = () => {
            dragRef.current = null;
            document.removeEventListener("mousemove", move);
            document.removeEventListener("mouseup", up);
          };
          document.addEventListener("mousemove", move);
          document.addEventListener("mouseup", up);
          return;
        }
        handleMouseDown(e);
        const move = (ev: MouseEvent) => handleMouseMove(ev);
        const up = () => {
          handleMouseUp();
          document.removeEventListener("mousemove", move);
          document.removeEventListener("mouseup", up);
        };
        document.addEventListener("mousemove", move);
        document.addEventListener("mouseup", up);
      };
      return {
        onMouseDown,
        role: "separator" as const,
        tabIndex: -1,
      };
    },
    [size, min, max, axis, handleMouseDown, handleMouseMove, handleMouseUp],
  );

  return [size, bindHandle];
}
