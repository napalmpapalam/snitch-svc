# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Run
```bash
# Build locally
go build -o snitch-svc github.com/napalmpapalam/snitch-svc

# Run the service (requires environment variables)
export DISCORD_TOKEN="your-discord-bot-token"
export TELEGRAM_TOKEN="your-telegram-bot-token"
./snitch-svc run all

# Build with Docker
docker build -t snitch-svc .

# Run with Docker
docker run -e DISCORD_TOKEN="your-token" -e TELEGRAM_TOKEN="your-token" snitch-svc
```

### Development
```bash
# Install dependencies
go mod download

# Tidy dependencies
go mod tidy

# Run with local config (ensure tokens are set in environment)
KV_VIPER_FILE=config.local.yaml ./snitch-svc run all
```

## Architecture

This service monitors Discord voice channel activity and sends notifications to Telegram when users join or leave specific channels.

### Core Components

1. **Discord Bot** (`discordgo`): Listens to voice state updates in a specific guild
2. **Telegram Bot** (`telegram-bot-api/v5`): Sends formatted notifications to a configured chat
3. **Voice State Cache**: In-memory cache with mutex protection to track user states and handle channel switches correctly
4. **Configuration System**: Uses `distributed_lab/kit` for config management with environment variable support

### Service Flow

1. Service registers Discord voice state update handler
2. Voice state cache tracks users to distinguish between:
   - User joining from outside any channel
   - User switching between channels  
   - User leaving to no channel
3. For tracked channels, sends Telegram notification with:
   - User's nickname (or username if no nickname)
   - Random emoji (special handling for user "rilein1" who gets "🧛")
   - Channel name and join/leave action

### Key Files

- `internal/services/snitch/main.go`: Core service logic and event handling
- `internal/config/`: Configuration interfaces for Discord and Telegram
- `internal/cli/main.go`: CLI setup using kingpin

## Important Implementation Details

### Voice State Caching
The service maintains an in-memory cache of voice states to correctly handle channel switches. Without this cache, a channel switch would appear as two separate events (leave + join).

### Configuration
- Tokens must be provided via environment variables: `DISCORD_TOKEN` and `TELEGRAM_TOKEN`
- Channel IDs and guild ID are configured in `config.yaml`
- Uses Viper for configuration with environment variable override support

### Error Handling
- Uses `distributed_lab/running.WithBackOff` for automatic reconnection
- Graceful shutdown on SIGTERM/SIGINT signals

## Development Guidelines

### Adding New Features
- Follow the existing clean architecture pattern
- Add new configuration options to the appropriate config interface
- Use the existing logging framework (`distributed_lab/logan`)

### Security Notes
- Never commit tokens or sensitive data
- Always use environment variables for secrets
- Current `config.local.yaml` contains tokens and should be in .gitignore

### Testing
No test infrastructure currently exists. When adding tests:
- Create `*_test.go` files alongside implementation
- Mock Discord and Telegram APIs for unit tests
- Consider adding integration tests with test Discord/Telegram accounts