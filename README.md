# offshore-desktop

Native desktop wrapper (macOS & Windows) for [offshore](https://offshore.pressed.press), built with **Tauri v2** and a **Nuitka** Python sidecar.

This repository contains **no copied Python source** — it builds the sidecar from `offshore-podcast` (either a local checkout during development or a pinned PyPI release).

## Features
- Dedicated native window (WKWebView on macOS, WebView2 on Windows)
- Minimize to menu bar / system tray on window close so scheduled episode releases keep running, with **Open Offshore** and **Quit Offshore** actions
- Native OS folder dialogs via `tauri-plugin-dialog` (automatically used by `offshore`'s web UI)
- Single-instance focus (re-launching focuses the existing window)
- External links automatically open in your default browser

## Prerequisites
- Rust + Cargo (`rustc`, `cargo`)
- Tauri v2 CLI: `cargo install tauri-cli --version "^2" --locked`
- Python 3.11+ with `offshore-podcast` and `nuitka` installed (for building the sidecar)

## Development (fast loop, no Nuitka rebuild needed)
Point `OFFSHORE_DEV_PYTHON` at a Python interpreter that has `offshore` installed (or let it auto-detect `../.offshoredev/bin/python`):

```bash
cd offshore-desktop
cargo tauri dev
```

## Building a macOS `.app` and `.dmg`
1. Build the standalone Nuitka sidecar:
   ```bash
   ./scripts/build_sidecar.sh
   ```
   *(Pass `PYTHON_BIN=/path/to/python` to override the default `../.offshoredev/bin/python`.)*

2. Build the Tauri app bundle & installer:
   ```bash
   cargo tauri build
   ```
   Outputs are written to `src-tauri/target/release/bundle/`.

## Building on Windows
1. Build the standalone Nuitka sidecar:
   ```powershell
   .\scripts\build_sidecar.ps1
   ```
2. Build the `.msi` / NSIS installer:
   ```powershell
   cargo tauri build
   ```
