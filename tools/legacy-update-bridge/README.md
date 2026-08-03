# Legacy PlayFusion update bridge

PlayFusion 1.0 uses the Kazeta+ ZIP updater protocol. Each public release that
still supports 1.0 must therefore publish a ZIP whose filename and single root
directory match, alongside the signed cumulative `.pfu` package used by
PlayFusion 1.0.2 and later.

The ZIP must contain `upgrade-to-plus.sh`, the `.pfu`, its `.sha256` and `.sig`,
the matching public key, and the protected updater bootstrap files. The bridge
verifies the signed package before it changes the system, installs the modern
transactional updater, and then delegates the update to that helper.

Never include the private signing key in the repository, ZIP, release assets,
installer image, or console filesystem.

PlayFusion 1.0 invokes the extracted bridge through `sudo`. Its compatibility
patch must therefore install a sudoers rule bound to both the exact extracted
script path and that release script's SHA-256 digest. Never authorize a wildcard
path or an arbitrary script under `/tmp`. The digest rule changes for every
legacy ZIP release and is no longer needed after the modern updater is installed.
