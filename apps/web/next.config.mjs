// One static export holds the website (/, /pricing/, /th/... ; headers in
// vercel.json), the browser client (/app/, /pill/) and the phone app
// (/phone/). A person's own daemon serves the same files for the client
// (crates/polyph-daemon/src/web.rs) and sends its root to /app/.
/** @type {import('next').NextConfig} */
const config = {
  output: "export",
  trailingSlash: true,
  transpilePackages: ["@polyph/ui"],
  images: { unoptimized: true },
  poweredByHeader: false,
};
export default config;
