import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Localizer } from "@polyph/ui/i18n";
import { describe, expect, it } from "vitest";
import { claimFor, fileClaims } from "../src/claims";
import { polyphHome, readConn } from "../src/daemon";
import { panelHtml } from "../src/panel";

describe("finding the daemon", () => {
  it("uses the same home as the CLI on each platform", () => {
    expect(polyphHome("/x", {}, "linux", "/home/a")).toBe("/x");
    expect(polyphHome("", { POLYPH_HOME: "/p" }, "linux", "/home/a")).toBe("/p");
    expect(polyphHome("", {}, "linux", "/home/a")).toBe("/home/a/.config/polyph");
    expect(polyphHome("", { XDG_CONFIG_HOME: "/cfg" }, "linux", "/home/a")).toBe("/cfg/polyph");
    expect(polyphHome("", {}, "darwin", "/Users/a")).toBe(
      "/Users/a/Library/Application Support/dev.polyph.polyph",
    );
    expect(polyphHome("", { APPDATA: "C:/R" }, "win32", "C:/U")).toBe(
      join("C:/R", "polyph", "polyph", "config"),
    );
  });

  it("reads the port and token, and refuses half-written files", () => {
    const d = mkdtempSync(join(tmpdir(), "polyph-vsc-"));
    expect(readConn(d)).toBeNull();
    writeFileSync(join(d, "daemon.json"), JSON.stringify({ port: 47850 }));
    writeFileSync(join(d, "api.token"), "short");
    expect(readConn(d)).toBeNull();
    writeFileSync(join(d, "api.token"), "a".repeat(64) + "\n");
    expect(readConn(d)).toEqual({ port: 47850, token: "a".repeat(64) });
  });
});

describe("claims", () => {
  const members = [
    { member: "ann", name: "Ann", hue: 255, me: true },
    { member: "bo", name: "Bo", hue: 170, me: false },
  ];
  const claim = (path: string, member: string, id: string) => ({
    id: `c-${id}`,
    target: { path, symbol: null },
    owner: { id, member, device: "d", kind: "claude", label: `${member}/claude` },
    reason: "refunds",
    expires_at: 0,
  });

  it("colors each claim in its holder's voice and marks the person's own", () => {
    const fc = fileClaims(
      [claim("src/lib.rs", "bo", "a1"), claim("src/api/", "ann", "a2")],
      members,
      "dark",
    );
    expect(fc[0]).toMatchObject({ path: "src/lib.rs", label: "bo/claude", mine: false });
    expect(fc[0]!.color).toMatch(/^oklch\(0\.\d{3} 0\.120 170\)$/);
    expect(fc[1]!.mine).toBe(true);
    expect(claimFor(fc, "src/lib.rs")?.label).toBe("bo/claude");
    expect(claimFor(fc, "src\\api\\routes.rs")?.label).toBe("ann/claude");
    expect(claimFor(fc, "src/main.rs")).toBeUndefined();
  });
});

describe("the panel", () => {
  it("allows only its own files and the daemon's socket", () => {
    const webview = {
      cspSource: "vscode-resource:",
      asWebviewUri: (u: { path: string }) => ({ toString: () => `vscode-resource:${u.path}` }),
    };
    const html = panelHtml(
      webview as never,
      { path: "/ext" } as never,
      { port: 4100, token: "t".repeat(64) },
      "dark",
      new Localizer("ja"),
    );
    const csp = /Content-Security-Policy" content="([^"]+)"/.exec(html)![1]!;
    expect(csp).toContain("default-src 'none'");
    expect(csp).toContain("connect-src ws://127.0.0.1:4100");
    expect(csp).not.toMatch(/https?:/);
    expect(html).toContain('lang="ja"');
    expect(html).toContain("vscode-resource:/ext/dist/webview/webview.js");
    const nonces = [...html.matchAll(/nonce="([^"]+)"/g)].map((m) => m[1]);
    expect(nonces.length).toBe(2);
    expect(csp).toContain(`'nonce-${nonces[0]}'`);
  });
});
