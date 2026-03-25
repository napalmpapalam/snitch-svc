# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build and Run
```bash
# Development build
cargo build

# Release build (LTO enabled, panic=abort)
cargo build --release

# Run the service (requires environment variables)
export DISCORD_TOKEN="your-discord-bot-token"
export TELEGRAM_TOKEN="your-telegram-bot-token"
cargo run -- run all

# Lint (strict — treat warnings as errors)
cargo clippy -- -D warnings

# Format
cargo fmt

# Run tests
cargo test

# Build with Docker
docker build -t snitch-svc .

# Run with Docker
docker run -e DISCORD_TOKEN="your-token" -e TELEGRAM_TOKEN="your-token" snitch-svc
```

### Development
```bash
# Run with local config (ensure tokens are set in environment)
CONFIG=config.local.yaml cargo run -- run all
```

## Architecture

This service monitors Discord voice channel activity and sends notifications to Telegram when users join or leave specific channels.

### Core Components

1. **Discord Bot** (`serenity`): Listens to voice state updates in a specific guild
2. **Telegram Bot** (`teloxide`): Sends formatted notifications to a configured chat
3. **Voice State Cache**: `Arc<RwLock<HashMap<UserId, VoiceState>>>` to track user states and handle channel switches correctly
4. **Configuration System**: `serde` + YAML deserialization with environment variable override support
5. **CLI**: `clap` for command parsing and subcommand routing

### Service Flow

1. Service registers Discord voice state update handler
2. Voice state cache tracks users to distinguish between:
   - User joining from outside any channel
   - User switching between channels
   - User leaving to no channel
3. For tracked channels, sends Telegram notification with:
   - User's nickname (or username if no nickname)
   - Random emoji (special handling for specific users)
   - Channel name and join/leave action
   - List of current members in the channel

### Reference: Go Implementation

The original Go implementation is preserved in `golang/` for reference during the rewrite.

## Important Implementation Details

### Voice State Caching
The service maintains an in-memory cache (`Arc<RwLock<HashMap>>`) of voice states to correctly handle channel switches. Without this cache, a channel switch would appear as two separate events (leave + join). On startup, the cache is populated from current guild voice states.

### Telegram Message Management
The service tracks sent Telegram message IDs per channel. When a channel's member list changes, the old message is deleted and a new one is sent, keeping the chat clean.

### Configuration
- Tokens must be provided via environment variables: `DISCORD_TOKEN` and `TELEGRAM_TOKEN`
- Channel IDs and guild ID are configured in `config.yaml`
- `config.local.yaml` is for local development and must be in `.gitignore`

### Error Handling
- Use `eyre::Result` with `wrap_err()` for all fallible operations
- No `unwrap()`, `expect()`, or `panic!()` in production code
- Automatic reconnection with backoff on Discord disconnection
- Graceful shutdown on SIGTERM/SIGINT signals

## Development Guidelines

### Key Principles
- Write idiomatic Rust — use ownership, borrowing, and the type system
- Keep files small and focused (<200 lines)
- Use `&str` over `String` when possible; avoid unnecessary `.clone()`
- Use iterators over explicit loops
- No wildcard imports
- No indexing/slicing — use `.get()` and safe alternatives
- Validate all input data
- Test after every meaningful change

### Git Conventions
Commit message format: `type: description`

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `ci`, `build`, `style`

### Dependency Management
All dependencies in root `Cargo.toml`:
- Disable `default-features` by default
- Enable only required features explicitly
- Sub-packages use workspace dependencies via `{ workspace = true }`

### Documentation
- Document public functions with `///` doc comments
- Add examples in doc comments with ` ```rust ` blocks
- Use `#![deny(missing_docs)]` in `lib.rs`

## Coding Style

### Formatting
- **rustfmt** for enforcement — always run `cargo fmt` before committing
- **clippy** for lints — `cargo clippy -- -D warnings` (treat warnings as errors)
- 4-space indent (rustfmt default)
- Max line width: 100 characters (rustfmt default)

### Immutability
- Use `let` by default; only use `let mut` when mutation is required
- Prefer returning new values over mutating in place
- Use `Cow<'_, T>` when a function may or may not need to allocate

```rust
use std::borrow::Cow;

// GOOD — immutable by default, new value returned
fn normalize(input: &str) -> Cow<'_, str> {
    if input.contains(' ') {
        Cow::Owned(input.replace(' ', "_"))
    } else {
        Cow::Borrowed(input)
    }
}

// BAD — unnecessary mutation
fn normalize_bad(input: &mut String) {
    *input = input.replace(' ', "_");
}
```

### Naming
- `snake_case` for functions, methods, variables, modules, crates
- `PascalCase` (UpperCamelCase) for types, traits, enums, type parameters
- `SCREAMING_SNAKE_CASE` for constants and statics
- Lifetimes: short lowercase (`'a`, `'de`) — descriptive names for complex cases (`'input`)

### Ownership and Borrowing
- Borrow (`&T`) by default; take ownership only when you need to store or consume
- Never clone to satisfy the borrow checker without understanding the root cause
- Accept `&str` over `String`, `&[T]` over `Vec<T>` in function parameters
- Use `impl Into<String>` for constructors that need to own a `String`

```rust
// GOOD — borrows when ownership isn't needed
fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

// GOOD — takes ownership in constructor via Into
fn new(name: impl Into<String>) -> Self {
    Self { name: name.into() }
}

// BAD — takes String when &str suffices
fn word_count_bad(text: String) -> usize {
    text.split_whitespace().count()
}
```

### Error Handling Style
- Use `Result<T, E>` and `?` for propagation — never `unwrap()` in production code
- **Libraries**: define typed errors with `thiserror`
- **Applications**: use `eyre` for flexible error context
- Add context with `.wrap_err("failed to ...")?`
- Reserve `unwrap()` / `expect()` for tests and truly unreachable states

```rust
// Library error with thiserror
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config format: {0}")]
    Parse(String),
}

// Application error with eyre
use eyre::{Result, WrapErr};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .wrap_err_with(|| format!("failed to read {path}"))?;
    toml::from_str(&content)
        .wrap_err_with(|| format!("failed to parse {path}"))
}
```

### Iterators Over Loops
Prefer iterator chains for transformations; use loops for complex control flow:

```rust
// GOOD — declarative and composable
let active_emails: Vec<&str> = users.iter()
    .filter(|u| u.is_active)
    .map(|u| u.email.as_str())
    .collect();

// GOOD — loop for complex logic with early returns
for user in &users {
    if let Some(verified) = verify_email(&user.email)? {
        send_welcome(&verified)?;
    }
}
```

### Module Organization
Organize by domain, not by type:

```text
src/
├── main.rs
├── lib.rs
├── auth/           # Domain module
│   ├── mod.rs
│   ├── token.rs
│   └── middleware.rs
├── orders/         # Domain module
│   ├── mod.rs
│   ├── model.rs
│   └── service.rs
└── db/             # Infrastructure
    ├── mod.rs
    └── pool.rs
```

### Visibility
- Default to private; use `pub(crate)` for internal sharing
- Only mark `pub` what is part of the crate's public API
- Re-export public API from `lib.rs`

## Patterns

### Repository Pattern with Traits

```rust
pub trait OrderRepository: Send + Sync {
    fn find_by_id(&self, id: u64) -> Result<Option<Order>, StorageError>;
    fn find_all(&self) -> Result<Vec<Order>, StorageError>;
    fn save(&self, order: &Order) -> Result<Order, StorageError>;
    fn delete(&self, id: u64) -> Result<(), StorageError>;
}
```

Concrete implementations handle storage details (Postgres, SQLite, in-memory for tests).

### Service Layer

Business logic in service structs; inject dependencies via constructor:

```rust
pub struct OrderService {
    repo: Box<dyn OrderRepository>,
    payment: Box<dyn PaymentGateway>,
}

impl OrderService {
    pub fn new(repo: Box<dyn OrderRepository>, payment: Box<dyn PaymentGateway>) -> Self {
        Self { repo, payment }
    }

    pub fn place_order(&self, request: CreateOrderRequest) -> eyre::Result<OrderSummary> {
        let order = Order::from(request);
        self.payment.charge(order.total())?;
        let saved = self.repo.save(&order)?;
        Ok(OrderSummary::from(saved))
    }
}
```

### Newtype Pattern for Type Safety

```rust
struct UserId(u64);
struct OrderId(u64);

fn get_order(user: UserId, order: OrderId) -> eyre::Result<Order> {
    // Can't accidentally swap user and order IDs at call sites
    todo!()
}
```

### Enum State Machines

Model states as enums — make illegal states unrepresentable:

```rust
enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { session_id: String },
    Failed { reason: String, retries: u32 },
}

fn handle(state: &ConnectionState) {
    match state {
        ConnectionState::Disconnected => connect(),
        ConnectionState::Connecting { attempt } if *attempt > 3 => abort(),
        ConnectionState::Connecting { .. } => wait(),
        ConnectionState::Connected { session_id } => use_session(session_id),
        ConnectionState::Failed { retries, .. } if *retries < 5 => retry(),
        ConnectionState::Failed { reason, .. } => log_failure(reason),
    }
}
```

Always match exhaustively — no wildcard `_` for business-critical enums.

### Builder Pattern

Use for structs with many optional parameters:

```rust
pub struct ServerConfig {
    host: String,
    port: u16,
    max_connections: usize,
}

impl ServerConfig {
    pub fn builder(host: impl Into<String>, port: u16) -> ServerConfigBuilder {
        ServerConfigBuilder { host: host.into(), port, max_connections: 100 }
    }
}

pub struct ServerConfigBuilder { host: String, port: u16, max_connections: usize }

impl ServerConfigBuilder {
    pub fn max_connections(mut self, n: usize) -> Self { self.max_connections = n; self }
    pub fn build(self) -> ServerConfig {
        ServerConfig { host: self.host, port: self.port, max_connections: self.max_connections }
    }
}

// Usage: ServerConfig::builder("localhost", 8080).max_connections(200).build()
```

### Sealed Traits for Extensibility Control

```rust
mod private {
    pub trait Sealed {}
}

pub trait Format: private::Sealed {
    fn encode(&self, data: &[u8]) -> Vec<u8>;
}

pub struct Json;
impl private::Sealed for Json {}
impl Format for Json {
    fn encode(&self, data: &[u8]) -> Vec<u8> { todo!() }
}
```

### Traits and Generics

```rust
// Generic input, concrete output
fn read_all(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

// Trait objects for heterogeneous collections or plugin systems
trait Handler: Send + Sync {
    fn handle(&self, request: &Request) -> Response;
}

struct Router {
    handlers: Vec<Box<dyn Handler>>,
}
```

### Option Combinators Over Nested Matching

```rust
// GOOD — combinator chain
fn find_user_email(users: &[User], id: u64) -> Option<String> {
    users.iter()
        .find(|u| u.id == id)
        .map(|u| u.email.clone())
}
```

### Concurrency

**`Arc<Mutex<T>>` for shared mutable state:**

```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));
let handles: Vec<_> = (0..10).map(|_| {
    let counter = Arc::clone(&counter);
    std::thread::spawn(move || {
        let mut num = counter.lock().expect("mutex poisoned");
        *num += 1;
    })
}).collect();
```

**Channels for message passing:**

```rust
use std::sync::mpsc;

let (tx, rx) = mpsc::sync_channel(16); // Bounded channel with backpressure
drop(tx); // Close sender so rx iterator terminates

for msg in rx {
    println!("{msg}");
}
```

**Async with Tokio:**

```rust
use tokio::time::Duration;

async fn fetch_with_timeout(url: &str) -> eyre::Result<String> {
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(url),
    )
    .await
    .wrap_err("request timed out")?
    .wrap_err("request failed")?;

    response.text().await.wrap_err("failed to read body")
}
```

### API Response Envelope

```rust
#[derive(Debug, serde::Serialize)]
#[serde(tag = "status")]
pub enum ApiResponse<T: serde::Serialize> {
    #[serde(rename = "ok")]
    Ok { data: T },
    #[serde(rename = "error")]
    Error { message: String },
}
```

### Anti-Patterns to Avoid

```rust
// Bad: .unwrap() in production code
let value = map.get("key").unwrap();

// Bad: .clone() to satisfy borrow checker without understanding why
let data = expensive_data.clone();

// Bad: Using String when &str suffices
fn greet(name: String) { /* should be &str */ }

// Bad: Box<dyn Error> in libraries (use thiserror instead)
fn parse(input: &str) -> Result<Data, Box<dyn std::error::Error>> { todo!() }

// Bad: Ignoring must_use warnings
let _ = validate(input); // Silently discarding a Result

// Bad: Blocking in async context
async fn bad_async() {
    std::thread::sleep(Duration::from_secs(1)); // Blocks the executor!
    // Use: tokio::time::sleep(Duration::from_secs(1)).await;
}
```

### Quick Reference: Rust Idioms

| Idiom                               | Description                                                |
| ----------------------------------- | ---------------------------------------------------------- |
| Borrow, don't clone                 | Pass `&T` instead of cloning unless ownership is needed    |
| Make illegal states unrepresentable | Use enums to model valid states only                       |
| `?` over `unwrap()`                 | Propagate errors, never panic in library/production code   |
| Parse, don't validate               | Convert unstructured data to typed structs at the boundary |
| Newtype for type safety             | Wrap primitives in newtypes to prevent argument swaps      |
| Prefer iterators over loops         | Declarative chains are clearer and often faster            |
| `#[must_use]` on Results            | Ensure callers handle return values                        |
| `Cow` for flexible ownership        | Avoid allocations when borrowing suffices                  |
| Exhaustive matching                 | No wildcard `_` for business-critical enums                |
| Minimal `pub` surface               | Use `pub(crate)` for internal APIs                         |

## Security

### Secrets Management
- Never hardcode API keys, tokens, or credentials in source code
- Use environment variables: `std::env::var("API_KEY")`
- Fail fast if required secrets are missing at startup
- Keep `.env` files in `.gitignore`

```rust
// BAD
const API_KEY: &str = "sk-abc123...";

// GOOD — environment variable with early validation
fn load_api_key() -> eyre::Result<String> {
    std::env::var("PAYMENT_API_KEY")
        .wrap_err("PAYMENT_API_KEY must be set")
}
```

### Input Validation
- Validate all user input at system boundaries before processing
- Use the type system to enforce invariants (newtype pattern)
- Parse, don't validate — convert unstructured data to typed structs at the boundary

```rust
pub struct Email(String);

impl Email {
    pub fn parse(input: &str) -> Result<Self, ValidationError> {
        let trimmed = input.trim();
        let at_pos = trimmed.find('@')
            .filter(|&p| p > 0 && p < trimmed.len() - 1)
            .ok_or_else(|| ValidationError::InvalidEmail(input.to_string()))?;
        let domain = &trimmed[at_pos + 1..];
        if trimmed.len() > 254 || !domain.contains('.') {
            return Err(ValidationError::InvalidEmail(input.to_string()));
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
```

### Unsafe Code
- Minimize `unsafe` blocks — prefer safe abstractions
- Every `unsafe` block must have a `// SAFETY:` comment explaining the invariant
- Never use `unsafe` to bypass the borrow checker for convenience
- Prefer safe FFI wrappers around C libraries

```rust
// GOOD — safety comment documents ALL required invariants
let widget: &Widget = {
    // SAFETY: `ptr` is non-null, aligned, points to an initialized Widget,
    // and no mutable references or mutations exist for its lifetime.
    unsafe { &*ptr }
};
```

### Dependency Security
- Run `cargo audit` to scan for known CVEs in dependencies
- Run `cargo deny check` for license and advisory compliance
- Use `cargo tree` to audit transitive dependencies
- Keep dependencies updated — set up Dependabot or Renovate
- Minimize dependency count — evaluate before adding new crates

```bash
cargo audit
cargo deny check
cargo tree
cargo tree -d  # Show duplicates only
```

### Error Messages
- Never expose internal paths, stack traces, or database errors in API responses
- Log detailed errors server-side; return generic messages to clients
- Use `tracing` or `log` for structured server-side logging

## Testing

### Test Organization

```text
src/
├── lib.rs           # Unit tests in #[cfg(test)] modules
├── auth/
│   └── mod.rs       # #[cfg(test)] mod tests { ... }
└── orders/
    └── service.rs   # #[cfg(test)] mod tests { ... }
tests/               # Integration tests (each file = separate binary)
├── api_test.rs
├── db_test.rs
└── common/          # Shared test utilities
    └── mod.rs
```

Unit tests go inside `#[cfg(test)]` modules in the same file. Integration tests go in `tests/`.

### Unit Test Pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_user_with_valid_email() {
        let user = User::new("Alice", "alice@example.com").unwrap();
        assert_eq!(user.name, "Alice");
    }

    #[test]
    fn rejects_invalid_email() {
        let result = User::new("Bob", "not-an-email");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid email"));
    }
}
```

### Parameterized Tests

```rust
use rstest::rstest;

#[rstest]
#[case("hello", 5)]
#[case("", 0)]
#[case("rust", 4)]
fn test_string_length(#[case] input: &str, #[case] expected: usize) {
    assert_eq!(input.len(), expected);
}
```

### Async Tests

```rust
#[tokio::test]
async fn fetches_data_successfully() {
    let client = TestClient::new().await;
    let result = client.get("/data").await;
    assert!(result.is_ok());
}
```

### Mocking with mockall

```rust
pub trait UserRepository {
    fn find_by_id(&self, id: u64) -> Option<User>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::eq;

    mockall::mock! {
        pub Repo {}
        impl UserRepository for Repo {
            fn find_by_id(&self, id: u64) -> Option<User>;
        }
    }

    #[test]
    fn service_returns_user_when_found() {
        let mut mock = MockRepo::new();
        mock.expect_find_by_id()
            .with(eq(42))
            .times(1)
            .returning(|_| Some(User { id: 42, name: "Alice".into() }));

        let service = UserService::new(Box::new(mock));
        let user = service.get_user(42).unwrap();
        assert_eq!(user.name, "Alice");
    }
}
```

### Test Naming
- `creates_user_with_valid_email()`
- `rejects_order_when_insufficient_stock()`
- `returns_none_when_not_found()`

### Coverage
- Target 80%+ line coverage
- Use `cargo-llvm-cov` for coverage reporting
- Focus on business logic — exclude generated code and FFI bindings

```bash
cargo llvm-cov                       # Summary
cargo llvm-cov --html                # HTML report
cargo llvm-cov --fail-under-lines 80 # Fail if below threshold
```

### Testing Commands

```bash
cargo test                       # Run all tests
cargo test -- --nocapture        # Show println output
cargo test test_name             # Run tests matching pattern
cargo test --lib                 # Unit tests only
cargo test --test api_test       # Specific integration test
cargo test --doc                 # Doc tests only
```
