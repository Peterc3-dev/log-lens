# log-lens

Streaming log viewer TUI with regex filtering, search, and bookmarks.

## Features

- View multiple log files simultaneously or pipe from stdin
- Follow mode (`-f`) tails files in real time, auto-scrolling as new lines arrive
- Stackable regex filters — chain include (`/`) and exclude (`!`) patterns to drill down
- Regex search with `n` / `N` to jump between matches
- Bookmark lines of interest (`b`) and browse them later (`B`)
- Line wrap toggle (`w`)
- Copy current line to system clipboard (`y`, supports Wayland and X11)
- 100K-line ring buffer (configurable with `--buffer-size`)
- Handles stdin piping transparently via `/dev/tty`

## Install

```
cargo build --release
# binary at target/release/log-lens
```

## Usage

```
# view log files
log-lens /var/log/syslog /var/log/kern.log

# follow mode
log-lens -f app.log

# pipe from another command
journalctl -f | log-lens
dmesg --follow | log-lens
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Scroll up/down |
| `PgUp` / `PgDn` | Page scroll |
| `g` / `Home` | Jump to top |
| `G` / `End` | Jump to bottom |
| `f` | Toggle auto-scroll (follow) |
| `/` | Add include filter (regex) |
| `!` | Add exclude filter (regex) |
| `d` | Pop last filter |
| `Ctrl-F` | Search (regex) |
| `n` / `N` | Next / previous search match |
| `b` | Toggle bookmark on current line |
| `B` | Open bookmark list |
| `w` | Toggle line wrap |
| `y` | Copy current line to clipboard |
| `q` / `Esc` | Quit |

---
Built with Rust + ratatui
