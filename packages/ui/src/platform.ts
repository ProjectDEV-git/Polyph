// What differs between the desktop shell and the browser client.
export interface Platform {
  kind: "desktop" | "web" | "phone" | "vscode";
  /** Views this surface offers; all when unset. The phone has no terminals or merges. */
  views?: readonly string[];
  /** A join code to fill in (from a phone pairing link). */
  joinCode?: string;
  /** Shows a native notification (desktop) when the window is not focused. */
  notify?: (title: string, body: string) => void;
  /** Opens a URL in the person's browser. */
  openExternal?: (url: string) => void;
  /** Shows or hides the status pill window (desktop). */
  togglePill?: () => void;
}
