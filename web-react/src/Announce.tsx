// A visually-hidden live region. Each form drops its one-sentence summary here,
// so a screen reader announces the figure someone came for rather than the whole
// table on every keystroke.
export function Announce({ text }: { text: string }) {
  return (
    <p className="vh" role="status" aria-live="polite">
      {text}
    </p>
  );
}
