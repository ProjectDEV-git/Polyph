// Small inline icons in Lucide's style (1.5px strokes on a 24 grid).
const base = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

export function StatusIcon({
  kind,
}: {
  kind: "running" | "waiting" | "blocked" | "done" | "error";
}) {
  switch (kind) {
    case "running":
      return (
        <svg {...base}>
          <circle cx="12" cy="12" r="5" fill="currentColor" />
        </svg>
      );
    case "waiting":
      return (
        <svg {...base}>
          <path d="M18 11V6a2 2 0 0 0-4 0v5M14 10V4a2 2 0 0 0-4 0v6M10 10.5V6a2 2 0 0 0-4 0v8a8 8 0 0 0 16 0v-2a2 2 0 0 0-4 0" />
        </svg>
      );
    case "blocked":
      return (
        <svg {...base}>
          <rect x="3" y="8" width="18" height="8" rx="1" />
          <path d="M7 8l4 8M13 8l4 8M7 16v4M17 16v4" />
        </svg>
      );
    case "error":
      return (
        <svg {...base}>
          <path d="M7.9 2h8.2L22 7.9v8.2L16.1 22H7.9L2 16.1V7.9z" />
          <path d="M12 8v4M12 16h.01" />
        </svg>
      );
    default:
      return (
        <svg {...base}>
          <path d="M20 6 9 17l-5-5" />
        </svg>
      );
  }
}

export function Arrow({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 56 56"
      fill="none"
      stroke="currentColor"
      strokeWidth="4"
      className={className}
      aria-hidden
    >
      <path d="M8 28h38M30 12l16 16-16 16" strokeLinecap="square" />
    </svg>
  );
}
