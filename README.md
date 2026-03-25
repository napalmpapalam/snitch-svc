# snitch-svc

Discord voice channel activity monitor that sends notifications to Telegram.

## Overview

Snitch watches specific Discord voice channels for user activity and sends real-time notifications to a Telegram chat when users join, leave, or switch channels. Each notification includes the user's display name, a random emoji, the channel name, and a list of current members.

## Features

- Detects voice channel joins, leaves, and switches (not just raw events)
- Sends formatted HTML notifications to Telegram with member lists
- Tracks per-channel notification messages — replaces old messages to keep chat clean
- Persists message state via Telegram pinned messages (survives restarts with no external DB)
- Random emoji per notification (with special handling for specific users)
- Dry-run mode for testing without sending Telegram messages
- Graceful shutdown on SIGTERM/SIGINT

## Architecture

Two async tasks communicate via a bounded `mpsc` channel:

```
Discord Gateway ──→ [Discord Task] ──→ mpsc(32) ──→ [Telegram Task] ──→ Telegram API
                    (producer)                       (consumer)
```

- **Discord task** connects to the Discord gateway, listens for voice state updates, resolves display names from the guild cache, and sends `VoiceEvent`s to the channel.
- **Telegram task** receives events, formats HTML notifications, manages message lifecycle (delete old → send new), and persists state to a pinned message.

## Project Structure

```
src/
├── main.rs              # CLI, config loading, task orchestration, shutdown
├── config.rs            # YAML config deserialization, env var injection
├── emoji.rs             # Random emoji pool, special user handling
├── events.rs            # VoiceEvent enum, ChannelUpdate, MemberInfo
├── discord/
│   ├── mod.rs           # Discord bot setup, gateway connection
│   └── handler.rs       # Voice state update handler, cache management
└── telegram/
    ├── mod.rs           # Telegram bot setup, event receiver loop
    └── notifier.rs      # Message formatting, send/delete, state persistence
```

## Getting Started

### Prerequisites

- Rust toolchain (stable)
- A Discord bot token with voice state intents enabled
- A Telegram bot token and chat ID for notifications

### Configuration

Create `config.yaml` (or `config.local.yaml` for development):

```yaml
log:
  level: info
  filter:
    - "serenity=warn"

telegram:
  chat_id: <telegram-chat-id>
  state_chat_id: <telegram-state-chat-id>

discord:
  target_guild_id: "<discord-guild-id>"
  tracked_channels:
    - "<channel-id-1>"
    - "<channel-id-2>"
```

Tokens are provided via environment variables:

```bash
export DISCORD_TOKEN="your-discord-bot-token"
export TELEGRAM_TOKEN="your-telegram-bot-token"
```

### Running

```bash
# Run with default config.yaml
cargo run -- run all

# Run with local config
CONFIG=config.local.yaml cargo run -- run all

# Dry-run mode (logs instead of sending to Telegram)
cargo run -- run all --dry-run
```

### Docker

```bash
docker build -t snitch-svc .
docker run -e DISCORD_TOKEN="..." -e TELEGRAM_TOKEN="..." snitch-svc
```

## Development

```bash
cargo build                    # Development build
cargo build --release          # Release build (LTO, panic=abort)
cargo clippy -- -D warnings    # Lint (warnings are errors)
cargo fmt                      # Format
cargo test                     # Run tests
```

## CI

GitHub Actions runs on every push: format check, clippy lint, tests, and build. See [.github/workflows/ci.yml](.github/workflows/ci.yml).

## License

See [LICENSE](LICENSE).
