import type { NextConfig } from "next";

// Browser talks to the Rust API through this origin, so the HTTP-only
// session cookie stays first-party and no CORS is needed.
const apiUrl = process.env.API_URL ?? "http://127.0.0.1:8080";

// Sites allowed to iframe /embed/* (space-separated origins). Everything else can't be framed.
// ponytail: one global allowlist; per-integration origins need the API to serve the header.
const embedOrigins = process.env.EMBED_ALLOWED_ORIGINS ?? "*";

const nextConfig: NextConfig = {
  cacheComponents: true,
  partialPrefetching: true,
  turbopack: {
    rules: {
      "*.css": {
        loaders: ["@tailwindcss/turbopack"],
        as: "*.css",
      },
    },
  },
  async headers() {
    return [
      { source: "/:path*", headers: [{ key: "Content-Security-Policy", value: "frame-ancestors 'self'" }] },
      { source: "/embed/:path*", headers: [{ key: "Content-Security-Policy", value: `frame-ancestors ${embedOrigins}` }] },
      { source: "/sdk/:path*", headers: [{ key: "Access-Control-Allow-Origin", value: "*" }] },
    ];
  },
  async rewrites() {
    return [{ source: "/api/:path*", destination: `${apiUrl}/api/:path*` }];
  },
};

export default nextConfig;
