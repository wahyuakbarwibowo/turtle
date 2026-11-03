# Documentation

TurtleMeet is a self-hosted video conferencing service: a Rust API, the LiveKit SFU and a
Next.js web app. These pages are generated from the `docs/` folder of the repository.

| Page | Read it when you want to |
|---|---|
| [Deployment](deploy.md) | Run the stack locally with Docker or on a VPS with HTTPS, and create LiveKit keys |
| [Integrations](integrations.md) | Create meetings from your platform, embed calls with the SDK, receive webhooks |
| [API reference](api.md) | See every HTTP endpoint the web app uses |
| [Architecture](architecture.md) | Understand the design decisions and why they were made |
| [Security](security.md) | Check the security review findings and their status |
| [Load test](load-test.md) | See capacity measurements so far and how to run your own |
| [Testing](testing.md) | Run the test suites |

## Quick start

```bash
git clone https://github.com/wahyuakbarwibowo/turtle
cd turtle
make deploy-env   # deploy/.env with random secrets
make deploy-up    # http://localhost
```

For development without Docker (hot reload), see the
[README](https://github.com/wahyuakbarwibowo/turtle#quick-start-local-development).
