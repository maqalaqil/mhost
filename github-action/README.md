# mhost Deploy - GitHub Action

Deploy and manage processes with [mhost](https://mhostai.com) in your GitHub Actions workflows.

## Usage

### Basic Deploy

```yaml
- uses: maqalaqil/mhost-action@v1
  with:
    command: deploy production
    config: mhost.toml
```

### Deploy with Specific Version

```yaml
- uses: maqalaqil/mhost-action@v1
  with:
    command: deploy production
    version: '0.5.0'
    config: mhost.toml
```

### Remote Server Deploy

```yaml
- uses: maqalaqil/mhost-action@v1
  with:
    command: deploy production --server my-server
    config: mhost.toml
    server: my-server
    ssh-key: ${{ secrets.DEPLOY_SSH_KEY }}
```

### Start Processes

```yaml
- uses: maqalaqil/mhost-action@v1
  with:
    command: start all
    config: mhost.toml
```

## Inputs

| Input | Required | Default | Description |
|-------|----------|---------|-------------|
| `command` | Yes | - | mhost command to run (e.g., `deploy production`, `start all`) |
| `version` | No | `latest` | mhost version to install |
| `config` | No | - | Path to `mhost.toml` config file |
| `server` | No | - | Remote server name for cloud deploy |
| `ssh-key` | No | - | SSH private key for remote deployments |

## Full Workflow Example

```yaml
name: Deploy
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: maqalaqil/mhost-action@v1
        with:
          command: deploy production
          config: mhost.toml
          ssh-key: ${{ secrets.DEPLOY_SSH_KEY }}
```

## Requirements

- The action installs mhost automatically. No pre-installation needed.
- For remote deployments, provide an SSH key via the `ssh-key` input (store it as a GitHub secret).

## License

MIT
