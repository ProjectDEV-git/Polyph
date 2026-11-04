// The browser client is exported as static files that the person's own
// daemon serves (crates/polyph-daemon/src/web.rs); the website (M10) will
// share this app.
/** @type {import('next').NextConfig} */
const config = {
  output: "export",
  trailingSlash: true,
  transpilePackages: ["@polyph/ui"],
  images: { unoptimized: true },
  poweredByHeader: false,
};
export default config;
