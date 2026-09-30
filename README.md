# octoping

Forwards GitHub webhooks to Discord, with templated messages and GitHub → Discord user mentions.

Events: `pr.opened`, `pr.approved`, `push.default` (push to the default branch).

## Run

```sh
cp octoping.example.toml octoping.toml
GITHUB_WEBHOOK_SECRET=... DISCORD_WEBHOOK_URL=... cargo run --release -- octoping.toml
```

## GitHub setup

Repo or org → Settings → Webhooks → Add webhook:

- Payload URL: `https://<host>/webhook`
- Content type: `application/json`
- Secret: same as `GITHUB_WEBHOOK_SECRET`
- Events: Pushes, Pull requests, Pull request reviews

Failed upstream deliveries return 502, so you can redeliver them from GitHub's webhook page.
A redelivery resends to every upstream on that event, including ones that already succeeded.
