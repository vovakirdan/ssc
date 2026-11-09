# Desktop packages

Generic releases contain no activation credentials. The `Generic desktop releases`
workflow builds Linux x64 AppImage and Windows x64 binaries, creates tar.gz assets
and SHA-256 sidecars, and publishes them for version tags. A manual run only uploads
generic build artifacts. Windows includes Microsoft's Authenticode-verified
WebView2 runtime bootstrapper; the SSC installer itself is currently unsigned.

Personal installers are produced by the `Personal desktop installer` workflow in
the **private** ssc-connect repository. Inputs are platform and a published version
tag, never a person's name or a code. The restricted SSH command `issue-bootstrap`
returns one fresh profile into a private temporary file. Only the final installer
is uploaded as a private artifact, retained one day. Transfer it privately; its
unredeemed activation code is a bearer capability.

Local packaging uses an existing private bootstrap file (do not print its contents):

```sh
node scripts/packaging/build-personal.mjs --platform linux --payload /private/generic-payload --bootstrap /private/bootstrap.json --version 0.2.0 --output /private/ssc-setup.run
```

The Windows command uses `--platform windows`, an `.exe` output and an NSIS compiler
on PATH (or `--compiler /path/to/makensis`). Payload files are `ssc.AppImage` for
Linux, or `ssc.exe` and `MicrosoftEdgeWebview2Setup.exe` for Windows. The builder
rejects symlinked/oversized/expired/foreign profiles; Unix input must be mode0600.
Compiler output is intentionally not echoed because inputs contain a credential.

Linux `.run` installs without sudo into `$XDG_DATA_HOME/ssc/app` (default
`~/.local/share/ssc/app`), creates an applications-menu entry and an `uninstall`
command alongside the app. The application is installed, not run from an extracted
portable archive. AppImage uses extract-and-run, so FUSE is not required.

Windows NSIS installs for the current user into `%LOCALAPPDATA%\Programs\SSC`,
provides Start-menu and uninstall entries, and installs WebView2 automatically.
It protects the profile directory ACL before copying the bootstrap file.

Bootstrap locations match the client's app-data directory:

- Linux: `$XDG_DATA_HOME/com.vladimirkirdan.ssc/bootstrap.json`.
- Windows: `%APPDATA%\com.vladimirkirdan.ssc\bootstrap.json`.

The installer never overwrites `access.key`. It skips a bootstrap copy if the
nonsecret `access.active` marker exists. A fresh package can replace a bootstrap
whose activation has not completed, while retaining its stable retry key.
Uninstall preserves access; use the application reset action to remove it.

Run `node --test scripts/packaging/packaging.test.mjs` for profile rejection and
actual Linux installation/update/uninstall checks with fixture payloads. These
checks do not establish Windows runtime acceptance. Compile the NSIS template and
test a real installation on Windows separately.
