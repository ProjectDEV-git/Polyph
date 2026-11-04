// Loads the phone engine (crates/polyph-phone as wasm). The files are built
// by scripts/build-phone-wasm.mjs into public/phone/ and loaded at run time,
// so the web build does not need them to type-check.
import type { Engine } from "@polyph/ui";

export interface EngineModule {
  default: (opts: { module_or_path: string | URL }) => Promise<unknown>;
  generateDeviceSecret(): Uint8Array;
  PhoneEngine: {
    new (
      secret: Uint8Array,
      member: string | undefined,
      name: string,
      relay: string,
    ): Engine & {
      save(): Uint8Array;
    };
    restore(secret: Uint8Array, saved: Uint8Array, now: number): Engine;
  };
}

export async function loadEngine(): Promise<EngineModule> {
  const base = new URL("./", window.location.href);
  const url = new URL("engine.js", base).href;
  const m = (await import(
    /* webpackIgnore: true */ /* turbopackIgnore: true */ url
  )) as EngineModule;
  await m.default({ module_or_path: new URL("engine.wasm", base) });
  return m;
}

export function supported(): boolean {
  return (
    typeof WebAssembly === "object" &&
    typeof WebSocket === "function" &&
    typeof indexedDB === "object" &&
    !!globalThis.crypto?.subtle
  );
}
