# snitch-svc

Discord voice channel activity monitor that sends notifications to Telegram.

## Overview

Snitch watches specific Discord voice channels for user activity and sends real-time notifications to a Telegram chat when users join, leave, or switch channels. It tracks session durations, detects achievements, and sends weekly voice time leaderboards.

## Features

- **Voice tracking** — detects joins, leaves, and channel switches
- **Combined message** — single Telegram message showing all active channels and members
- **Duration tracking** — shows how long each member has been in voice, survives restarts
- **Achievements** — fun badges triggered by events (Party starter, Speed run, Boomerang, etc.) and durations (Marathon, Overtime, etc.)
- **Weekly digest** — Monday 9 AM (Ukraine time) leaderboard with voice time stats
- **State persistence** — all state stored in a Telegram pinned message (no external DB)
- **Periodic updates** — duration tick edits the message in place every N minutes
- **InitialState reconciliation** — correctly handles users who joined/left while the service was down
- **Graceful shutdown** on SIGTERM/SIGINT

## Architecture

Two async tasks communicate via a bounded `mpsc` channel:

```
Discord Gateway ──→ [Discord Task] ──→ mpsc(32) ──→ [Telegram Task] ──→ Telegram API
                    (producer)                       (consumer)
```

- **Discord task** connects to the Discord gateway, listens for voice state updates, resolves display names from the guild cache, and sends `VoiceEvent`s to the channel.
- **Telegram task** receives events, maintains voice sessions, detects achievements, formats HTML notifications, manages message lifecycle (delete old → send new on events, edit in place on ticks), and persists state to a pinned message.

## Project Structure

```
src/
├── main.rs              # CLI, config loading, task orchestration, shutdown
├── config.rs            # YAML config deserialization, env var injection
├── emoji.rs             # Random emoji pool
├── events.rs            # VoiceEvent enum, ChannelUpdate, newtypes (Username, DisplayName, ChannelName)
├── discord/
│   ├── mod.rs           # Discord bot setup, gateway connection
│   └── handler.rs       # Voice state update handler, initial state emission
└── telegram/
    ├── mod.rs           # Telegram bot setup, tokio::select! loop (events + tick)
    ├── service.rs       # TelegramService: session management, event/tick handling, state persistence
    ├── state.rs         # Data model: SessionInfo, WeeklyStats, PersistentState, type aliases
    ├── format.rs        # Message formatting: event, status, digest, duration
    └── achievements.rs  # Achievement detection: immediate (join/leave) and duration-based (tick)
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
  duration_tick_minutes: 5

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

## License

See [LICENSE](LICENSE).
