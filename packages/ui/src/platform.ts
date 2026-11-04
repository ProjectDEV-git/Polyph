// What differs between the desktop shell and the browser client.
export interface Platform {
  kind: "desktop" | "web";
  /** Shows a native notification (desktop) when the window is not focused. */
  notify?: (title: string, body: string) => void;
  /** Opens a URL in the person's browser. */
  openExternal?: (url: string) => void;
  /** Shows or hides the status pill window (desktop). */
  togglePill?: () => void;
}
