// Finds the person's daemon the way the CLI does (crates/polyph-daemon/src/
// paths.rs): POLYPH_HOME, else the platform config directory; reads its
// port from daemon.json and the API token from api.token; starts it with
// `polyph daemon start` when it is not running.
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

/** The directory `directories::ProjectDirs::from("dev", "polyph", "polyph")` picks. */
export function defaultHome(
  platform: NodeJS.Platform = process.platform,
  env: NodeJS.ProcessEnv = process.env,
  home: string = homedir(),
): string {
  if (env.POLYPH_HOME) return env.POLYPH_HOME;
  switch (platform) {
    case "darwin":
      return join(home, "Library", "Application Support", "dev.polyph.polyph");
    case "win32":
      return join(env.APPDATA ?? join(home, "AppData", "Roaming"), "polyph", "polyph", "config");
    default:
      return join(env.XDG_CONFIG_HOME || join(home, ".config"), "polyph");
  }
}

export interface Conn {
  port: number;
  token: string;
}

export async function readConn(home: string): Promise<Conn> {
  const info = JSON.parse(await readFile(join(home, "daemon.json"), "utf8")) as { port: number };
  const token = (await readFile(join(home, "api.token"), "utf8")).trim();
  return { port: info.port, token };
}

async function healthy(c: Conn): Promise<boolean> {
  try {
    const r = await fetch(`http://127.0.0.1:${c.port}/healthz`, {
      signal: AbortSignal.timeout(1500),
    });
    return r.ok;
  } catch {
    return false;
  }
}

/** The running daemon, started if needed. */
export async function ensureDaemon(home: string, command: string): Promise<Conn> {
  try {
    const c = await readConn(home);
    if (await healthy(c)) return c;
  } catch {}
  await new Promise<void>((ok, no) =>
    execFile(command, ["daemon", "start"], { env: { ...process.env, POLYPH_HOME: home } }, (e) =>
      e ? no(e) : ok(),
    ),
  );
  for (let i = 0; i < 50; i++) {
    try {
      const c = await readConn(home);
      if (await healthy(c)) return c;
    } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("daemon did not start");
}

export function wsUrl(c: Conn): string {
  return `ws://127.0.0.1:${c.port}/ws?token=${encodeURIComponent(c.token)}`;
}
