import { useEffect, useRef, useState } from "react";

// Smoothly interpolates between integers (cents or ppm) when the target changes.
// The duration is intentionally short: it draws the eye without making the user
// wait for a figure they need to read now.
export function useAnimatedNumber(target: number, duration = 220): number {
  const [value, setValue] = useState(target);
  const startRef = useRef({ from: target, to: target, start: 0 });

  useEffect(() => {
    if (value === target) return;
    startRef.current = { from: value, to: target, start: performance.now() };
    let raf = 0;
    const tick = (now: number) => {
      const elapsed = now - startRef.current.start;
      const progress = Math.min(elapsed / duration, 1);
      // easeOutCubic: quick start, gentle landing
      const eased = 1 - Math.pow(1 - progress, 3);
      const current = Math.round(
        startRef.current.from +
          (startRef.current.to - startRef.current.from) * eased,
      );
      setValue(current);
      if (progress < 1) {
        raf = requestAnimationFrame(tick);
      }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [target]);

  return value;
}
