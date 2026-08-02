# Public installer source

`install.sh` is the installer embedded in the PlayFusion 1.0.1 public image.
It deploys the combined PlayFusion system snapshot, installs clean factory
data, generates unique SSH host keys, prepares persistent writable user data,
reseals the system deployment, and validates the result before reporting
success.

The installer intentionally keeps the deployed operating-system snapshot
read-only. PlayFusion's writable Kazeta+ application data is stored at:

```text
/var/kazeta/user-data/kazeta-plus
```

The application-facing path remains:

```text
/home/gamer/.local/share/kazeta-plus
```

through a validated directory symlink. This design keeps settings, themes,
profiles, and related user data persistent without unlocking the complete
deployment.
