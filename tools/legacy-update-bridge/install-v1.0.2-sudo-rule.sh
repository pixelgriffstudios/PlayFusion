#!/usr/bin/env bash
set -euo pipefail

SCRIPT=/tmp/PlayFusion-legacy-update-v1.0.2/upgrade-to-plus.sh
RULE=/etc/sudoers.d/playfusion-legacy-v102

test "$(id -u)" -eq 0
test -f "$SCRIPT"
digest=$(sha256sum "$SCRIPT" | awk '{ print $1 }')
printf 'gamer ALL=(root) NOPASSWD: sha256:%s %s\n' "$digest" "$SCRIPT" > "$RULE"
chown root:root "$RULE"
chmod 0440 "$RULE"
visudo -cf "$RULE"
sudo -u gamer sudo -n -l "$SCRIPT" >/dev/null
printf 'Authorized the signed PlayFusion 1.0.2 legacy bridge only.\n'
