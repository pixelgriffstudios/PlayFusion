#!/usr/bin/env bash
set -euo pipefail

PAYLOAD_ROOT=${1:?Usage: build-playfusion-update.sh PAYLOAD_ROOT VERSION MINIMUM_VERSION PRIVATE_KEY OUTPUT_DIR [POST_INSTALL] [DELETE_LIST]}
VERSION=${2:?Missing target version}
MINIMUM_VERSION=${3:?Missing minimum version}
PRIVATE_KEY=${4:?Missing private signing key}
OUTPUT_DIR=${5:?Missing output directory}
POST_INSTALL=${6:-}
DELETE_LIST=${7:-}

test -d "$PAYLOAD_ROOT"
test -r "$PRIVATE_KEY"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ "$MINIMUM_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
command -v bsdtar >/dev/null
command -v openssl >/dev/null
command -v sha256sum >/dev/null

mkdir -p "$OUTPUT_DIR"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

package_name="PlayFusion-update-v${VERSION}.pfu"
package="$OUTPUT_DIR/$package_name"

bsdtar --zstd -cf "$work/payload.tar.zst" -C "$PAYLOAD_ROOT" .
payload_hash=$(sha256sum "$work/payload.tar.zst" | awk '{ print $1 }')
cat > "$work/manifest.toml" <<EOF
product = "PlayFusion"
format = 1
version = "$VERSION"
minimum_version = "$MINIMUM_VERSION"
restart = "reboot"
payload_sha256 = "$payload_hash"
EOF

members=(manifest.toml payload.tar.zst)
if test -n "$POST_INSTALL"; then
    test -f "$POST_INSTALL"
    bash -n "$POST_INSTALL"
    install -m 0755 "$POST_INSTALL" "$work/post-install"
    members+=(post-install)
fi
if test -n "$DELETE_LIST"; then
    test -f "$DELETE_LIST"
    install -m 0644 "$DELETE_LIST" "$work/delete.list"
    members+=(delete.list)
fi

rm -f -- "$package" "$package.sha256" "$package.sig"
(cd "$work" && bsdtar -cf "$package" "${members[@]}")
hash=$(sha256sum "$package" | awk '{ print $1 }')
printf '%s  %s\n' "$hash" "$package_name" > "$package.sha256"
openssl dgst -sha256 -sign "$PRIVATE_KEY" -out "$package.sig" "$package"
openssl dgst -sha256 -verify <(openssl pkey -in "$PRIVATE_KEY" -pubout) \
    -signature "$package.sig" "$package" >/dev/null

printf '%s\n' "$package" "$package.sha256" "$package.sig"
sha256sum "$package"
