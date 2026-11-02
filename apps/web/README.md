# TurtleMeet web

The Next.js frontend: landing page, sign-in/sign-up, and the meeting room built on
`@livekit/components-react`. See the [root README](../../README.md) for setup and
`make dev` to run it together with the API and LiveKit.

```bash
npm run dev        # http://localhost:3000 (expects the API at API_URL, default :8080)
npm run typecheck  # generates Next route types, then runs tsc
npm run lint
npm run e2e        # browser E2E, see ../../docs/testing.md
```

`/api/*` is proxied to the Rust API (`next.config.ts`), so the session cookie stays
same-origin. In-call UI lives in `src/components/call/`, and the API client and Zod
schemas live in `src/lib/api.ts`.
