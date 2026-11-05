#!/usr/bin/env node
// Copies Excalidraw's fonts into an app's public folder so whiteboards never
// load fonts from a CDN. Usage: copy-excalidraw-assets.mjs <public dir>
import { cpSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const target = resolve(process.argv[2] ?? "public", "excalidraw");
const root = fileURLToPath(new URL("..", import.meta.url));
const fonts = join(root, "packages/ui/node_modules/@excalidraw/excalidraw/dist/prod/fonts");
if (!existsSync(join(target, "fonts"))) cpSync(fonts, join(target, "fonts"), { recursive: true });
