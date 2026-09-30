# octoping

Forwards GitHub webhooks to Discord, with templated messages and GitHub → Discord user mentions.

Events are defined in the config: a name, the GitHub event (`X-GitHub-Event`), an optional MiniJinja `when` expression over the payload, a template and upstreams. The example ships `pr_opened`, `pr_approved`, `pr_merged` and `push_default` (push to the default branch). `ping` gets `pong`.

## Run

```sh
cp octoping.example.yaml octoping.yaml
GITHUB_WEBHOOK_SECRET=... DISCORD_WEBHOOK_URL=... cargo run --release -- --config octoping.yaml
```

The config file expands `$VAR`/`${VAR}` from the environment before YAML parsing, so values with YAML syntax (`#`, `: `, quotes) need care. `OCTOPING__<PATH>` env vars override scalar config values (strings, numbers); lists like `to` must be set in the file.

Logging: `--log-level`/`RUST_LOG` (default `info`, per-target like `warn,octoping=debug`) and `--log-format`/`LOG_FORMAT` (`compact` default, `full`, `json`, `pretty`). Each request is logged with its GitHub event and delivery id.

The config file is re-read every `reload_secs` (default 10) and swapped in when it changes, so templates, events, users and upstreams update without a restart. An invalid file is logged and the previous config kept. `listen` changes, and env vars (including `${VAR}` values), need a restart. SIGTERM finishes in-flight deliveries before exiting.

In Kubernetes, mount the ConfigMap as a directory (not `subPath`, which never updates) and point `--config` at the file in it.

## GitHub setup

Repo or org → Settings → Webhooks → Add webhook:

- Payload URL: `https://<host>/webhook`
- Content type: `application/json`
- Secret: same as `GITHUB_WEBHOOK_SECRET`
- Events: Pushes, Pull requests, Pull request reviews

Failed upstream deliveries return 502, so you can redeliver them from GitHub's webhook page.
A redelivery resends to every upstream on that event, including ones that already succeeded.

## Docker

```sh
docker run -p 8080:8080 \
  -v "$PWD/octoping.yaml:/etc/octoping/octoping.yaml:ro" \
  -e GITHUB_WEBHOOK_SECRET=... -e DISCORD_WEBHOOK_URL=... \
  ghcr.io/<owner>/octoping:main
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy and tests, then pushes the image to GHCR on `main` (`:main`, `:sha-…`) and on `v*` tags (`:1.2.3`). Pull requests only build it.
