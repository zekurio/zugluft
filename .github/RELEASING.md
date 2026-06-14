# Releasing

zugluft has two release channels:

- **Stable**: tagged `vX.Y.Z` and marked as the latest GitHub Release.
- **Nightly**: tagged `vX.Y.0-nightly.YYYYMMDD.<run>` and marked as a
  prerelease. Nightlies use the next minor after the latest stable tag. For
  example, after `v0.1.0`, nightly builds use `v0.2.0-nightly...`.

GitHub Release assets are intentionally limited to:

- `zugluft-setup-vX.Y.Z-windows-x64.exe`
- `checksums.txt`

The installer contains the GUI, CLI, Windows service and
`zugluft-lhm-bridge.dll`. During installation it also runs the PawnIO driver
installer when PawnIO is missing, then registers and starts the `zugluft`
service from the installed path.

## Stable Releases

1. Update the workspace version in `Cargo.toml` if it is not already correct.
2. After `main` is ready, create and push a stable tag:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

3. The **Release** workflow builds the Windows installer, generates checksums,
   asks GitHub to generate changelog notes, prepends zugluft install notes, and
   publishes the GitHub Release.

You can also run **Release** manually with:

- `channel`: `stable`
- `version`: `X.Y.Z` or `vX.Y.Z`

## Nightly Releases

Nightlies run automatically every day at `03:17 UTC`. The workflow skips the
scheduled run when `main` has not changed since the last nightly tag.
Nightly installers use the faster `release-fast` Cargo profile; stable
installers use the fully optimized `release` profile.

To cut a nightly manually, run **Release** with:

- `channel`: `nightly`
- `version`: empty

## Recovery

If a build fails before the GitHub Release is created, fix the issue and rerun
the failed workflow jobs. If a release was already published with bad assets,
delete the release and tag, then rerun from the corrected commit.
