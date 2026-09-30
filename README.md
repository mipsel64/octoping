# octoping

Forwards GitHub webhooks to Discord, with templated messages and GitHub → Discord user mentions.

Events: `pr_opened`, `pr_approved`, `push_default` (push to the default branch). `ping` gets `pong`.

## Run

```sh
cp octoping.example.yaml octoping.yaml
OCTOPING__SECRET=... OCTOPING__UPSTREAMS__DEV__URL=... cargo run --release -- --config octoping.yaml
```

`OCTOPING__<PATH>` env vars override any config value.

## GitHub setup

Repo or org → Settings → Webhooks → Add webhook:

- Payload URL: `https://<host>/webhook`
- Content type: `application/json`
- Secret: same as `OCTOPING__SECRET`
- Events: Pushes, Pull requests, Pull request reviews

Failed upstream deliveries return 502, so you can redeliver them from GitHub's webhook page.
A redelivery resends to every upstream on that event, including ones that already succeeded.

## Docker

```sh
docker run -p 8080:8080 \
  -v "$PWD/octoping.yaml:/etc/octoping/octoping.yaml:ro" \
  -e OCTOPING__SECRET=... -e OCTOPING__UPSTREAMS__DEV__URL=... \
  ghcr.io/<owner>/octoping:main
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests, then pushes the image to GHCR on `main` (`:main`, `:sha-…`) and on `v*` tags (`:1.2.3`). Pull requests only build it.
