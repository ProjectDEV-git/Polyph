// The Polyph mark: three parallel strokes of different length and offset that
// converge toward one baseline (DESIGN.md). Drawn on a 24-unit grid.
export function Mark({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
      className={className}
    >
      <path d="M4 7.5 L13 19.5" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
      <path
        d="M9 4.5 L15.6 19.5"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        opacity="0.78"
      />
      <path
        d="M14.5 6 L18.2 19.5"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
        opacity="0.56"
      />
    </svg>
  );
}
