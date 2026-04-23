#!/usr/bin/env bash
# uninstall-service.sh — Stop and remove the Kerr systemd user service.
#
# Usage: ./scripts/uninstall-service.sh

set -euo pipefail

SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
SERVICE_FILE="$SYSTEMD_USER_DIR/kerr.service"

die() { echo "Error: $*" >&2; exit 1; }

command -v systemctl >/dev/null 2>&1 \
    || die "systemd is not available on this system."

echo "Stopping and disabling Kerr service..."

systemctl --user stop    kerr.service 2>/dev/null || true
systemctl --user disable kerr.service 2>/dev/null || true

if [[ -f "$SERVICE_FILE" ]]; then
    rm "$SERVICE_FILE"
    echo "  Removed: $SERVICE_FILE"
else
    echo "  Service file not found (already removed?)."
fi

systemctl --user daemon-reload

echo ""
echo "Kerr service removed."
echo "Note: Your login session (~/.config/kerr/session.json) was not modified."
