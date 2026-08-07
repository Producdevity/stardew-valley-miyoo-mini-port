# Desktop app

The desktop app prepares Stardew Valley for Miyoo Mini. It can use an existing
Steam installation, a folder or ZIP, or download the supported compatibility
build after a Steam Mobile QR sign-in. It checks the game files and runs the
same preparation script included with the release.

The setup log can be copied to the clipboard or saved as a text file from the
app.

## Development

Install Node.js, pnpm, Rust and the Tauri system dependencies. Before building
an installer, put the matching port archive in the project's ignored
`releases` directory.

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

Steam downloads use [DepotDownloader](https://github.com/SteamRE/DepotDownloader).
The app verifies the download and uses Steam Mobile QR sign-in. DepotDownloader's
GPL-2.0 license is kept beside the downloaded tool.

The app uses installed copies of Mono and Docker on macOS and Linux. On Windows
it runs them in the default WSL distribution, so Mono must be installed there
and Docker Desktop's WSL integration must be enabled. There's is a guide link
that helps with the installation of system software you are missing.

Run all checks with `pnpm check`. Build a local installer with
`pnpm tauri:build`; this verifies and bundles the release archive first.

## Release builds

Publish the port archive with the matching GitHub release. Then run the
`Setup app` workflow manually. It builds Windows x64, macOS ARM64 and Intel,
and Linux x64 and ARM64 packages. Sign and notarize the macOS builds and sign
the Windows installer before publishing them.
