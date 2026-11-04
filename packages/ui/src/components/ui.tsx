// Small primitives in the shadcn/ui manner: plain elements, Tailwind classes,
// DESIGN.md sizes (32px controls, 6px radius, 13px text).
import clsx from "clsx";
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";
import { forwardRef } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-ink hover:brightness-110 active:brightness-95",
  secondary: "border border-line bg-s2 text-ink hover:bg-s3",
  ghost: "text-ink-2 hover:bg-s3 hover:text-ink",
  danger: "border border-line text-error hover:bg-s3",
};

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: "sm" | "md" }
>(function Button({ variant = "secondary", size = "md", className, type = "button", ...p }, ref) {
  return (
    <button
      ref={ref}
      type={type}
      className={clsx(
        "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-control font-medium transition-[background-color,filter] duration-150 ease-out-app disabled:opacity-60",
        size === "md" ? "h-8 px-3 text-sm" : "h-7 px-2 text-xs",
        variants[variant],
        className,
      )}
      {...p}
    />
  );
});

export function IconButton({
  label,
  className,
  children,
  ...p
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={clsx(
        "inline-flex size-8 shrink-0 items-center justify-center rounded-control text-ink-2 hover:bg-s3 hover:text-ink",
        className,
      )}
      {...p}
    >
      {children}
    </button>
  );
}

const field =
  "h-8 w-full rounded-control border border-line bg-s2 px-2.5 text-sm text-ink placeholder:text-ink-2 focus:border-accent focus:outline-none";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...p }, ref) {
    return <input ref={ref} className={clsx(field, className)} {...p} />;
  },
);

export const Textarea = forwardRef<
  HTMLTextAreaElement,
  TextareaHTMLAttributes<HTMLTextAreaElement>
>(function Textarea({ className, ...p }, ref) {
  return (
    <textarea
      ref={ref}
      className={clsx(field, "h-auto min-h-8 resize-none py-1.5 leading-5", className)}
      {...p}
    />
  );
});

export function Select({ className, ...p }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select className={clsx(field, "pe-7", className)} {...p} />;
}

export function Label({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) {
  return (
    <label htmlFor={htmlFor} className="mb-1.5 block text-sm font-medium text-ink">
      {children}
    </label>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-line px-1 font-mono text-xs text-ink-2">{children}</kbd>
  );
}

/** A section heading inside a view: 13px, secondary ink, sentence case. */
export function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="flex h-8 items-center justify-between gap-2">
      <h2 className="text-sm font-medium text-ink-2">{children}</h2>
      {aside}
    </div>
  );
}

/** An empty state: the serif headline (DESIGN.md) and one sentence. */
export function Empty({
  title,
  children,
  action,
}: {
  title?: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-start gap-2 rounded-card border border-dashed border-line px-4 py-4">
      {title && <p className="font-serif text-2xl text-ink">{title}</p>}
      <p className="max-w-prose text-sm text-ink-2">{children}</p>
      {action}
    </div>
  );
}

/** A loading placeholder at the real size of what it stands for. */
export function Bar({ w = "60%", h = 12 }: { w?: string; h?: number }) {
  return <span aria-hidden className="block rounded bg-s3" style={{ width: w, height: h }} />;
}

export function Rows({ n = 4, pitch = 28 }: { n?: number; pitch?: number }) {
  return (
    <div aria-busy className="flex flex-col">
      {Array.from({ length: n }, (_, i) => (
        <div key={i} className="flex items-center" style={{ height: pitch }}>
          <Bar w={`${70 - i * 9}%`} />
        </div>
      ))}
    </div>
  );
}
