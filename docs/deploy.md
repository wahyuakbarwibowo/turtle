# Deployment

The whole stack runs from `deploy/compose.yml`: Traefik, web (Next.js), API (Rust),
LiveKit, PostgreSQL and Valkey. Migrations run when the API starts.

```
browser ──:80/:443──▶ Traefik ──/api──▶ api :8080 ──▶ postgres, redis
                         ├──────────▶ web :3000
                         └─rtc.*────▶ livekit :7880 (signaling)
browser ──7881/tcp, 7882/udp, 3478/udp──▶ livekit (media, TURN)
```

## Local test (http://localhost)

Needs Docker with Compose v2.

```bash
make deploy-env    # deploy/.env with random secrets (gitignored)
make deploy-up     # build images and start everything
make deploy-logs
```

Open http://localhost. Browsers allow camera and microphone on `localhost` without
HTTPS. LiveKit is published on `ws://localhost:7880`, and `node_ip` is `127.0.0.1`, so
only this machine can join. Stop with `make deploy-down` (the database volume is kept).

API key for an integration:

```bash
docker compose -f deploy/compose.yml --env-file deploy/.env exec api \
  server integration create myapp --webhook-url https://myapp.example/hook
```

## Production (VPS, Ubuntu 24.04)

Sizing: start with 2 vCPU / 4 GB. Capacity is not measured yet (Phase 8).

1. **DNS:** point `meet.example.com` and `rtc.example.com` to the server's public IP.
2. **Firewall:** open `80/tcp`, `443/tcp`, `7881/tcp`, `7882/udp`, `3478/udp`.
3. **Docker:** `curl -fsSL https://get.docker.com | sh`
4. **Config:** clone the repo, run `make deploy-env`, then edit `deploy/.env` and use the
   production block from `.env.example`:
   - `DOMAIN`, `RTC_DOMAIN`, `PUBLIC_WEB_URL=https://…`, `LIVEKIT_PUBLIC_URL=wss://rtc…`
   - `COOKIE_SECURE=true`, `ENTRYPOINT=websecure`, `ACME_EMAIL`
   - `LIVEKIT_USE_EXTERNAL_IP=true` and an empty `LIVEKIT_NODE_IP`, so LiveKit
     advertises the public IP
   - `TURN_ENABLED=true`, `TURN_DOMAIN=rtc.example.com` for users behind strict NAT
   - `EMBED_ALLOWED_ORIGINS` with your integrators' origins
5. **Start:** `make deploy-up TLS=1`. Traefik gets certificates from Let's Encrypt.

Update: `git pull && make deploy-up TLS=1`.

Backup: `docker compose -f deploy/compose.yml --env-file deploy/.env exec postgres pg_dump -U turtlemeet turtlemeet > backup.sql`

## Notes

- `EMBED_ALLOWED_ORIGINS` is baked into the web image; rebuild after changing it.
- TURN uses LiveKit's built-in server on UDP 3478 instead of a separate coturn. TURN over
  TLS (for networks that only allow 443) is not set up yet.
- Media uses one UDP port (7882) and one TCP port (7881) instead of a large port range,
  which suits Docker's port mapping. Revisit if load tests in Phase 8 show limits.
