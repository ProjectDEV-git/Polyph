// What differs between the desktop shell and the browser client.
export interface Platform {
  /** "phone": the PWA, a room member of its own with no daemon behind it. */
  kind: "desktop" | "web" | "phone";
  /** Shows a native notification (desktop) when the window is not focused. */
  notify?: (title: string, body: string) => void;
  /** Opens a URL in the person's browser. */
  openExternal?: (url: string) => void;
  /** Shows or hides the status pill window (desktop). */
  togglePill?: () => void;
  /** Phone: forget the room on this device (it can be linked again). */
  leave?: () => void;
}
