import { useEffect, useState } from "react";

/**
 * §5.8 breakpoints, in one place so the arithmetic is checkable:
 *
 *   7-day planner needs   48 + (7 × 110) = 818px
 *   rail + sidebar        52 + 260       = 312px
 *   rail + collapsed      52 + 48        = 100px
 *   detail column                          360px
 */
export const BREAK_DETAIL_COLUMN = 1280;
export const BREAK_SIDEBAR_COLLAPSE = 1130;

export function useViewportWidth(): number {
  const [width, setWidth] = useState(() =>
    typeof window === "undefined" ? 1440 : window.innerWidth,
  );
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}
