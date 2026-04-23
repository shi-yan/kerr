#!/usr/bin/env bash
# install-service.sh — Register `kerr serve` as a systemd user service.
#
# Prerequisites:
#   1. Run `kerr login` at least once so a session exists.
#   2. Either pass --name <alias> OR create the autostart config file:
#        ~/.config/kerr/autostart.json  →  { "register": "your-alias" }
#
# Usage:
#   ./scripts/install-service.sh [--name <alias>] [--binary <path>]
#   ./scripts/install-service.sh --help

set -euo pipefail

# ── Config paths ───────────────────────────────────────────────────────────────
KERR_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/kerr"
SESSION_FILE="$KERR_CONFIG_DIR/session.json"
AUTOSTART_CONFIG="$KERR_CONFIG_DIR/autostart.json"
SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
SERVICE_FILE="$SYSTEMD_USER_DIR/kerr.service"

# XDG state dir is the correct location for persistent log files from a user
# service (as opposed to XDG_CONFIG_HOME for config or XDG_CACHE_HOME for
# transient data).  Falls back to ~/.local/state when XDG_STATE_HOME is unset.
KERR_LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/kerr"
KERR_LOG_FILE="$KERR_LOG_DIR/server.log"

# ── Helpers ────────────────────────────────────────────────────────────────────
die()  { echo "Error: $*" >&2; exit 1; }
info() { echo "  $*"; }

json_field() {
    # json_field <file> <key>  — extract a top-level string value without jq
    local file="$1" key="$2" value=""
    if command -v python3 >/dev/null 2>&1; then
        value=$(python3 - "$file" "$key" <<'PYEOF'
import json, sys
try:
    data = json.load(open(sys.argv[1]))
    print(data.get(sys.argv[2], ""))
except Exception:
    print("")
PYEOF
        )
    else
        # Fallback: basic grep-based extraction (no nested objects)
        value=$(grep -oE "\"${key}\"[[:space:]]*:[[:space:]]*\"[^\"]+\"" "$file" \
            | sed 's/.*":\s*"\(.*\)"/\1/' 2>/dev/null || true)
    fi
    printf '%s' "$value"
}

# ── Parse arguments ────────────────────────────────────────────────────────────
REGISTER_NAME=""
KERR_BIN_OVERRIDE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --name)
            [[ $# -ge 2 ]] || die "--name requires an argument."
            REGISTER_NAME="$2"; shift 2;;
        --name=*)
            REGISTER_NAME="${1#--name=}"; shift;;
        --binary)
            [[ $# -ge 2 ]] || die "--binary requires an argument."
            KERR_BIN_OVERRIDE="$2"; shift 2;;
        --binary=*)
            KERR_BIN_OVERRIDE="${1#--binary=}"; shift;;
        -h|--help)
            cat <<'HELP'
Usage: install-service.sh [OPTIONS]

Register 'kerr serve --register <alias>' as a systemd user service that
auto-starts on machine boot.

Prerequisites:
  1. Run 'kerr login' first to create a valid session.
  2. Provide the registration alias via --name or in the autostart config:
       ~/.config/kerr/autostart.json
     Contents: { "register": "your-alias" }

Options:
  --name <alias>   Registration alias passed to 'kerr serve --register'.
                   Takes precedence over the autostart config file.
  --binary <path>  Path to the kerr binary (auto-detected if not set).
  -h, --help       Show this help message.

After installation:
  Status     : systemctl --user status kerr
  Live logs  : journalctl --user -u kerr -f
  Log file   : ~/.local/state/kerr/server.log  (or $XDG_STATE_HOME/kerr/)
  Stop       : systemctl --user stop kerr
  Disable    : systemctl --user disable kerr
  Remove     : ./scripts/uninstall-service.sh
HELP
            exit 0;;
        *)
            die "Unknown argument: '$1'. Run with --help for usage.";;
    esac
done

# ── Preflight: systemd ─────────────────────────────────────────────────────────
command -v systemctl >/dev/null 2>&1 \
    || die "systemd is not available on this system. Cannot install a service."

# ── Preflight: kerr binary ─────────────────────────────────────────────────────
if [[ -n "$KERR_BIN_OVERRIDE" ]]; then
    KERR_BIN="$KERR_BIN_OVERRIDE"
    [[ -x "$KERR_BIN" ]] \
        || die "Specified binary is not executable: $KERR_BIN"
else
    KERR_BIN=""
    for candidate in \
        "$(command -v kerr 2>/dev/null || true)" \
        "$HOME/.cargo/bin/kerr" \
        "$HOME/.local/bin/kerr" \
        "/usr/local/bin/kerr"; do
        if [[ -n "$candidate" && -x "$candidate" ]]; then
            KERR_BIN="$candidate"
            break
        fi
    done
    [[ -n "$KERR_BIN" ]] \
        || die "Cannot find 'kerr' binary. Build it, install it, or pass --binary <path>."
fi

# ── Preflight: user login ──────────────────────────────────────────────────────
[[ -f "$SESSION_FILE" ]] \
    || die "Not logged in. Run 'kerr login' first, then re-run this script."

SESSION_ID="$(json_field "$SESSION_FILE" "session_id")"
[[ -n "$SESSION_ID" ]] \
    || die "Session file exists but has no valid session_id. Run 'kerr login' again."

# ── Resolve registration alias ─────────────────────────────────────────────────
if [[ -z "$REGISTER_NAME" ]]; then
    if [[ ! -f "$AUTOSTART_CONFIG" ]]; then
        die "No registration alias provided and no autostart config found.
       Either:
         1. Pass:  --name <your-alias>
         2. Create $AUTOSTART_CONFIG with:
              { \"register\": \"your-alias\" }"
    fi

    REGISTER_NAME="$(json_field "$AUTOSTART_CONFIG" "register")"
    [[ -n "$REGISTER_NAME" ]] \
        || die "Key 'register' is missing or empty in $AUTOSTART_CONFIG"
fi

# ── Write systemd unit file ────────────────────────────────────────────────────
mkdir -p "$SYSTEMD_USER_DIR"
mkdir -p "$KERR_LOG_DIR"

cat > "$SERVICE_FILE" <<UNIT
[Unit]
Description=Kerr P2P Remote Shell Server
Documentation=https://github.com/shi-yan/kerr
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=$KERR_BIN serve --register $REGISTER_NAME --log $KERR_LOG_FILE
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
UNIT

info "Service file written: $SERVICE_FILE"
info "Log file location  : $KERR_LOG_FILE"

# ── Enable linger so service starts at boot, not just on login ─────────────────
if loginctl enable-linger "$(id -un)" 2>/dev/null; then
    info "Linger enabled for user '$(id -un)' — service will start at boot."
else
    echo ""
    echo "Warning: Could not enable linger. The service will only start when you log in,"
    echo "         not automatically at machine boot."
    echo "         To fix: sudo loginctl enable-linger $(id -un)"
fi

# ── Activate the service ───────────────────────────────────────────────────────
systemctl --user daemon-reload
systemctl --user enable kerr.service
systemctl --user start  kerr.service

echo ""
echo "Kerr service installed and started."
echo "  Alias      : $REGISTER_NAME"
echo "  Binary     : $KERR_BIN"
echo "  Log file   : $KERR_LOG_FILE"
echo "  Status     : systemctl --user status kerr"
echo "  Live logs  : journalctl --user -u kerr -f"
echo "  Log file   : tail -f $KERR_LOG_FILE"
echo "  Stop       : systemctl --user stop kerr"
echo "  Remove     : $(dirname "$0")/uninstall-service.sh"
