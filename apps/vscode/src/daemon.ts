// Finds the person's Polyph daemon: the same files the CLI uses
// (`daemon.json` for the port, `api.token`) in the Polyph home.
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** The Polyph home: a setting, `POLYPH_HOME`, else the platform config directory. */
export function polyphHome(
  setting: string,
  env = process.env,
  platform = process.platform,
  home = homedir(),
): string {
  if (setting) return setting;
  if (env.POLYPH_HOME) return env.POLYPH_HOME;
  // directories::ProjectDirs::from("dev", "polyph", "polyph").config_dir()
  if (platform === "darwin")
    return join(home, "Library", "Application Support", "dev.polyph.polyph");
  if (platform === "win32")
    return join(env.APPDATA ?? join(home, "AppData", "Roaming"), "polyph", "polyph", "config");
  return join(env.XDG_CONFIG_HOME || join(home, ".config"), "polyph");
}

export interface Conn {
  port: number;
  token: string;
}

/** The running daemon's port and token, or null when it is not set up. */
export function readConn(home: string): Conn | null {
  try {
    const info = JSON.parse(readFileSync(join(home, "daemon.json"), "utf8")) as { port?: number };
    const token = readFileSync(join(home, "api.token"), "utf8").trim();
    if (!info.port || token.length < 32) return null;
    return { port: info.port, token };
  } catch {
    return null;
  }
}

export async function healthy(port: number): Promise<boolean> {
  try {
    const r = await fetch(`http://127.0.0.1:${port}/healthz`, {
      signal: AbortSignal.timeout(1000),
    });
    return r.ok;
  } catch {
    return false;
  }
}

/** Starts the daemon with the `polyph` on PATH. */
export function startDaemon(home: string): Promise<boolean> {
  return new Promise((ok) => {
    const p = spawn("polyph", ["daemon", "start"], {
      env: { ...process.env, POLYPH_HOME: home },
      stdio: "ignore",
    });
    p.on("error", () => ok(false));
    p.on("exit", (code) => ok(code === 0));
  });
}
