# rog

Halfway between `tail -f` and `lnav`. A versatile log reader for filtering, monitoring, and serving log data.

## Modes

- **Tail mode** — Monitor files in real-time: `rog app.log error.log`
- **FIFO mode** — Create and read from a named pipe: `rog -f /tmp/myfifo`
- **Server mode** — Read stdin and serve over TCP: `some_program | rog -s`
- **Client mode** — Connect to a rog server: `rog -k` (localhost) or `rog -I 10.0.0.4`

## Filtering

- `-g REGEX` — Only show matching lines
- `-w REGEX` — Whole-word matching
- `-v REGEX` — Invert match (show non-matching lines)
- `-i` — Case-insensitive matching
- `-C NUM` / `-A NUM` / `-B NUM` — Context lines around matches

## Formatting

- `-r F1,F2,...` — Remove fields matching `key=value` from output
- `-m THEME` — Syntax highlighting theme (13 themes available; use `-c` to disable)
- `-u` — Truncate lines to terminal width
- `-o NUM` — Limit bytes per row

## Other Options

- `-d PORT` — TCP port (default: 19888)
- `-x REGEX` — Exclude files matching pattern
- `-H` — Parse tail-style headers (`==> file <==`) in stdin/socket input
- `-p CHARS` — Use preset args from `~/.config/rogrc`
- `-P` — Skip loading presets

## Examples

```bash
# Tail multiple files, grep for errors with context
rog *.log -g "^ERROR" -C3

# Pipe output through server, connect remotely
some_program | rog -s
rog -I 10.0.0.4

# Create a FIFO and tail it
rog -f /tmp/myfifo
# Then: echo "log line" > /tmp/myfifo

# Remove sensitive fields while monitoring
rog app.log -r password,token -m Nord
```

## Presets

Default config at `~/.config/rogrc`. Set persistent options:

```
default = -m Dracula -u
myapp.log = -g "^ERROR|^WARN" -C2
```

## Build

```bash
cargo build --release
```

### Static binary (musl)

```bash
pacman -S musl
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### Debug logging

```bash
cargo build --features debug_log
```

## Tests

```bash
cargo test
```
