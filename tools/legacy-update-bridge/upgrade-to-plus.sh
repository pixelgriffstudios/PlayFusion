#!/usr/bin/env bash
set -Eeuo pipefail

VERSION="1.0.3"
# A sudoers SHA-256 command digest can execute a script through /dev/fd/N.
# In that case BASH_SOURCE no longer identifies the extracted update folder.
# The legacy updater always extracts an asset into /tmp using the ZIP basename,
# so use that stable, versioned location and only fall back to BASH_SOURCE for
# manual/offline invocation outside the old updater.
EXPECTED_KIT_DIR="/tmp/PlayFusion-legacy-update-v${VERSION}"
if test -d "$EXPECTED_KIT_DIR"; then
    KIT_DIR=$EXPECTED_KIT_DIR
else
    KIT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
fi
PACKAGE="${KIT_DIR}/PlayFusion-update-v${VERSION}.pfu"
CHECKSUM="${PACKAGE}.sha256"
SIGNATURE="${PACKAGE}.sig"
PUBLIC_KEY="${KIT_DIR}/playfusion-update-public.pem"
BACKUP="/var/lib/playfusion-updater/legacy-bridge-backup-$(date +%Y%m%dT%H%M%S)"
BOOTSTRAP_ACTIVE=0

fail() {
    printf 'PlayFusion update failed: %s\n' "$*" >&2
    exit 1
}

restore_bootstrap() {
    test "$BOOTSTRAP_ACTIVE" -eq 1 || return 0
    set +e
    for relative in \
        usr/bin/playfusion-update-helper \
        usr/bin/playfusion-update-health \
        usr/bin/playfusion-game-exit-hotkey \
        etc/playfusion-update-public.pem \
        etc/playfusion-release \
        etc/systemd/system/playfusion-update-health.service \
        etc/sudoers.d/playfusion-update; do
        destination="/${relative}"
        if test -e "${BACKUP}/root/${relative}" || test -L "${BACKUP}/root/${relative}"; then
            mkdir -p -- "$(dirname -- "$destination")"
            cp -a -- "${BACKUP}/root/${relative}" "$destination"
        elif grep -Fqx -- "$relative" "${BACKUP}/created.list" 2>/dev/null; then
            rm -f -- "$destination"
        fi
    done
    systemctl daemon-reload >/dev/null 2>&1 || true
}

trap restore_bootstrap ERR

test "$(id -u)" -eq 0 || fail "the legacy bridge must run as root"
for path in "$PACKAGE" "$CHECKSUM" "$SIGNATURE" "$PUBLIC_KEY"; do
    test -s "$path" || fail "missing signed update component: $(basename -- "$path")"
done

expected=$(awk 'NR == 1 { print tolower($1) }' "$CHECKSUM")
actual=$(sha256sum "$PACKAGE" | awk '{ print $1 }')
test "${#expected}" -eq 64 && test "$expected" = "$actual" \
    || fail "SHA-256 verification failed"
openssl dgst -sha256 -verify "$PUBLIC_KEY" -signature "$SIGNATURE" "$PACKAGE" >/dev/null \
    || fail "signature verification failed"

if command -v frzr-unlock >/dev/null 2>&1; then
    frzr-unlock >/dev/null 2>&1 || fail "unable to unlock the system deployment"
fi

mkdir -p "$BACKUP/root"
: > "$BACKUP/created.list"
for relative in \
    usr/bin/playfusion-update-helper \
    usr/bin/playfusion-update-health \
    usr/bin/playfusion-game-exit-hotkey \
    etc/playfusion-update-public.pem \
    etc/playfusion-release \
    etc/systemd/system/playfusion-update-health.service \
    etc/sudoers.d/playfusion-update; do
    source="/${relative}"
    if test -e "$source" || test -L "$source"; then
        mkdir -p -- "${BACKUP}/root/$(dirname -- "$relative")"
        cp -a -- "$source" "${BACKUP}/root/${relative}"
    else
        printf '%s\n' "$relative" >> "$BACKUP/created.list"
    fi
done
BOOTSTRAP_ACTIVE=1

install -o root -g root -m 0755 "${KIT_DIR}/playfusion-update-helper" /usr/bin/playfusion-update-helper
install -o root -g root -m 0755 "${KIT_DIR}/playfusion-update-health" /usr/bin/playfusion-update-health
hotkey_temp=$(mktemp /usr/bin/.playfusion-game-exit-hotkey.update.XXXXXX)
install -o root -g root -m 0755 "${KIT_DIR}/playfusion-game-exit-hotkey" "$hotkey_temp"
mv -Tf -- "$hotkey_temp" /usr/bin/playfusion-game-exit-hotkey
install -o root -g root -m 0644 "$PUBLIC_KEY" /etc/playfusion-update-public.pem
install -o root -g root -m 0644 "${KIT_DIR}/playfusion-update-health.service" \
    /etc/systemd/system/playfusion-update-health.service
install -o root -g root -m 0440 "${KIT_DIR}/playfusion-update.sudoers" \
    /etc/sudoers.d/playfusion-update

if ! test -s /etc/playfusion-release; then
    printf 'PRODUCT=PlayFusion\nVERSION=1.0.0\n' > /etc/playfusion-release
    chown root:root /etc/playfusion-release
    chmod 0644 /etc/playfusion-release
fi

systemctl daemon-reload
systemctl enable playfusion-update-health.service >/dev/null
/usr/bin/playfusion-update-helper install "$PACKAGE" "$CHECKSUM" "$SIGNATURE"

BOOTSTRAP_ACTIVE=0
trap - ERR
printf 'PlayFusion %s is installed and protected by automatic rollback.\n' "$VERSION"
