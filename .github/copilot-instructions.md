# Copilot Instructions for TeamsTUI

## Project Overview

TeamsTUI is a Rust-based Terminal User Interface (TUI) for Microsoft Teams, built around keyboard-driven navigation. It enables users to view and interact with their Teams chats in a fast, distraction-free terminal environment.

### Key Features

- OAuth2 authentication using Device Code Flow
- View and navigate Teams chats
- Send and receive messages
- Vim-style keyboard navigation
- Token persistence (automatic refresh, no re-authentication needed)
- Modern, colorful terminal UI using ratatui

## Technology Stack

- **Language**: Rust (Edition 2021)
- **Async Runtime**: tokio (full features)
- **HTTP Client**: reqwest (with JSON and rustls-tls)
- **Serialization**: serde and serde_json
- **TUI Framework**: ratatui (v0.29.0) with crossterm (v0.28.1)
- **Date/Time**: chrono
- **Error Handling**: anyhow
- **Configuration**: dotenv, dirs

## Project Structure

```
teams-tui/
├── src/
│   ├── main.rs      # Entry point, terminal setup, main event loop
│   ├── app.rs       # Application state management
│   ├── ui.rs        # UI rendering logic using ratatui
│   ├── auth.rs      # OAuth2 authentication (Device Code Flow)
│   ├── api.rs       # Microsoft Graph API client
│   └── config.rs    # Configuration constants
├── Cargo.toml       # Rust dependencies
├── .env.example     # Example environment configuration
├── README.md        # User documentation
├── AZURE_SETUP.md   # Azure AD app registration guide
├── LICENSE          # MIT License
└── assets/
    └── images/      # Screenshots and images
```

## Key Components

### main.rs

- Entry point for the application
- Handles terminal setup and cleanup (raw mode, alternate screen)
- Implements the main event loop (`run_app`)
- Manages async message loading and chat refresh
- Processes keyboard events

### app.rs

- Defines the `App` struct containing all application state
- Manages chats, messages, user info, scroll position
- Handles input mode toggle and buffer management

### ui.rs

- Renders the TUI layout using ratatui
- Split layout: chat list (30%) | messages (70%)
- Handles message formatting, HTML stripping, emoji extraction
- Implements text wrapping and scroll management
- Visual differentiation between sent/received messages

### auth.rs

- Implements Microsoft OAuth2 Device Code Flow
- Token persistence to `~/.config/teams-tui/token.json`
- Automatic token refresh with offline_access scope
- Client ID configuration from env vars or config file

### api.rs

- Microsoft Graph API client
- Endpoints: `/me`, `/me/chats`, `/chats/{id}/messages`, `/chats/{id}/members`
- User profile caching to `~/.config/teams-tui/profile.json`
- Message sending capability

### config.rs

- Application-wide constants
- Configuration directory name: `teams-tui`

## Development Guidelines

### Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release
```

### Running

```bash
# Run in development
cargo run

# Run release binary
./target/release/teams-tui
```

### Configuration

The application requires a Client ID from Azure AD. Configure it via:

1. **Config file** (preferred): `~/.config/teams-tui/config.json`
   ```json
   {
     "client_id": "your-client-id-here"
   }
   ```

2. **Environment variable**: Set `CLIENT_ID` in `.env` file or environment

### Testing

Currently, the project does not have automated tests. When adding tests:

- Use `#[tokio::test]` for async tests
- Mock HTTP responses for API tests
- Consider integration tests for UI components

### Linting

```bash
# Check for issues
cargo clippy

# Format code
cargo fmt

# Check formatting
cargo fmt --check
```

## Code Style and Conventions

### Rust Guidelines

- Follow standard Rust naming conventions (snake_case for functions/variables, PascalCase for types)
- Use `anyhow::Result` for error handling
- Leverage serde for JSON serialization with `#[serde(rename = "camelCase")]` for API fields
- Use async/await for all I/O operations
- Prefer `Option<T>` over nullable types

### UI Conventions

- Use `Color::Cyan` for chat metadata/headers
- Use `Color::Green` for user's own messages and status
- Use `Color::Yellow` with `BOLD` for selected items
- Maintain consistent spacing with empty lines between message groups

### Error Handling

- Use `anyhow::bail!` for early returns with errors
- Use `context()` to add context to errors
- Log warnings with `eprintln!` for non-fatal issues
- Return `Result<T>` from all fallible functions

## Authentication Flow

1. Application checks for existing token in `~/.config/teams-tui/token.json`
2. If valid token exists (with 5-minute buffer), use it
3. If token expired, attempt refresh using refresh_token
4. If refresh fails or no token exists, initiate Device Code Flow:
   - Request device code from Microsoft
   - Display code and URL to user
   - Poll for token completion
   - Save token with expiration timestamp

### Required Permissions (Delegated)

- `User.Read` - Read user profile
- `Chat.Read` - Read chats
- `Chat.ReadWrite` - Send messages
- `offline_access` - Refresh tokens

## Keyboard Controls

| Key | Mode | Action |
|-----|------|--------|
| `↑` / `k` | Normal | Move to previous chat |
| `↓` / `j` | Normal | Move to next chat |
| `PgUp` | Normal | Scroll messages up |
| `PgDn` | Normal | Scroll messages down |
| `i` | Normal | Enter input mode |
| `q` | Normal | Quit application |
| `Enter` | Input | Send message |
| `Esc` | Input | Cancel input |
| `Backspace` | Input | Delete character |

## Working with the Codebase

### Adding New API Endpoints

1. Define response structs in `api.rs` with serde derives
2. Create async function using reqwest client
3. Handle errors with `anyhow::bail!`
4. Update UI in `ui.rs` if displaying new data

### Modifying UI Layout

1. Update layout constraints in `ui.rs`
2. Create new widget rendering functions
3. Update state in `app.rs` if needed
4. Connect to event handling in `main.rs`

### Adding Keyboard Shortcuts

1. Add new `KeyCode` match arm in `run_app` (main.rs)
2. Add state fields to `App` if needed
3. Implement action logic
4. Update help text in UI blocks

## File Locations

- **User config**: `~/.config/teams-tui/config.json`
- **Auth token**: `~/.config/teams-tui/token.json`
- **User profile cache**: `~/.config/teams-tui/profile.json`

## Important Notes

- The application filters out meeting chats (only shows oneOnOne and group chats)
- Messages are auto-refreshed every 3 seconds
- HTML content in messages is stripped and converted to plain text
- Emoji tags are converted to their alt text representation
- The app uses Device Code Flow which doesn't require a redirect URI
