# snitch-svc

Discord voice channel activity monitor that sends notifications to Telegram when users join or leave tracked channels.

## Prerequisites

- **Rust 1.92+** — [Install via rustup](https://rustup.rs/)
- **Docker** (optional) — [Install Docker](https://docs.docker.com/get-docker/)
- A **Discord bot token** with voice state and message content intents
- A **Telegram bot token** with access to the target chat

## Installation

### From Source

```bash
git clone https://github.com/nickskyline/snitch-svc.git
cd snitch-svc
cargo build --release
```

### Verify

```bash
./target/release/snitch --help
```

### Docker

```bash
docker build -t snitch-svc .
```

## Usage

```bash
export DISCORD_TOKEN="your-discord-bot-token"
export TELEGRAM_TOKEN="your-telegram-bot-token"

# Run the service
snitch run all
```

With Docker:

```bash
docker run -e DISCORD_TOKEN="..." -e TELEGRAM_TOKEN="..." snitch-svc
```

## Configuration

The service reads from `config.yaml` (override with `CONFIG` env var):

```yaml
log:
  level: debug
  filter:
    - "serenity=warn"
    - "hyper_util=warn"

telegram:
  chat_id: -100123456789          # Telegram group to send notifications
  state_chat_id: -100987654321    # Telegram chat for state persistence
  duration_tick_minutes: 5        # Status update interval

discord:
  target_guild_id: "123456789"
  tracked_channels:               # Voice channels to monitor
    - "111111111"
    - "222222222"
  text_channels:                  # Text channels for blue post forwarding
    - id: "333333333"
      filter: "Class Tuning Incoming"
```

| Variable | Purpose |
|----------|---------|
| `DISCORD_TOKEN` | Discord bot authentication token |
| `TELEGRAM_TOKEN` | Telegram bot authentication token |
| `CONFIG` | Path to config file (default: `config.yaml`) |

## Architecture

Two async Tokio tasks connected by a bounded `mpsc` channel:

```
Discord Gateway ──▶ Discord Task ──▶ [mpsc channel] ──▶ Telegram Task ──▶ Telegram API
                   (event producer)                     (event consumer)
```

- **Discord task** — connects via serenity, listens for voice state updates and message embeds, sends events to the channel
- **Telegram task** — receives events, formats HTML notifications, manages message lifecycle, persists state to a pinned message

### Features

- Voice join/leave/switch notifications with member lists and session durations
- Weekly voice time statistics with Monday morning digests
- Achievement detection (PartyStarter, SpeedRun, Boomerang, Channel Hopper, Dynamic Duo, and more)
- Blue post forwarding — detects Discord embeds matching configured keywords and forwards to Telegram
- State persistence across restarts via pinned Telegram message
- Graceful shutdown on SIGINT/SIGTERM

## Development

```bash
# Debug build
cargo build

# Lint (strict — warnings are errors)
cargo clippy -- -D warnings

# Format
cargo fmt

# Run tests
cargo test

# Local config
CONFIG=config.local.yaml cargo run -- run all
```

## License

[MIT](LICENSE)
