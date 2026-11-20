# JARVIS for Windows

Run the x64 NSIS setup executable to install JARVIS for the current user. Launch **JARVIS** from Start. Windows Settings → Apps → Installed apps provides the uninstaller. The installed program is `JARVIS.exe`. The setup executable has a separate `-setup.exe` suffix.

The native window opens at up to 1600 × 900 logical pixels, constrained by the work area. Windows supplies the title bar, resizing, minimize, maximize and close controls. F11 or **Modo imersivo** toggles the conversation panel so the plasma view can fill the workspace. Escape restores the conversation. Tauri hosts the local Vite production assets in WebView2. Node.js and Vite are development dependencies.

## Build from the project root

Use Windows x64 with Node.js and npm, stable Rust for `x86_64-pc-windows-msvc`, the Visual Studio C++ x64 build tools, and a Windows SDK. The initial build needs network access for npm packages, Rust crates and Tauri's NSIS tools. PowerShell commands below assume Rust was installed for the current user.

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustc -vV
npm.cmd ci
npm.cmd run tauri -- dev
```

Development starts Vite on 127.0.0.1 port 5173 and opens the desktop window. Close the dev session before building the release.

```powershell
npm.cmd run tauri -- build -- --locked
```

The release command runs `npm run build`, embeds `dist/` into the executable, and bundles the current-user installer. Keep both `package-lock.json` and `src-tauri/Cargo.lock` with the source. The release binary is under `src-tauri/target/release/JARVIS.exe`, and the installer is under `src-tauri/target/release/bundle/nsis/`. The source icon at `src-tauri/icons/icon.ico` contains seven sizes from 16 to 256 pixels.

## Runtime and distribution

This configuration uses the Evergreen WebView2 Runtime. Setup can download Microsoft's bootstrapper when the runtime is missing. For a disconnected installation, provision the runtime beforehand or build a larger installer with `bundle.windows.webviewInstallMode.type` set to `offlineInstaller`. Tauri also supports distributing a fixed-version runtime. The packaged frontend loads local resources after installation. [Tauri Windows deployment options](https://v2.tauri.app/distribute/windows-installer/)

Public distribution can use an Authenticode certificate to sign the application and installer. This project has no signing credentials configured.

## Windows x64 assistant release · 8.0.0

The final Tauri build produced both files below. Both are unsigned. Paths are relative to the project root.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `src-tauri/target/release/JARVIS.exe` | 8,883,200 | `4DBDB6850E20FEB0FD1CD2C61EFF3E133F816FFEE6844E7F08833FF9E5BF2617` |
| `src-tauri/target/release/bundle/nsis/JARVIS_8.0.0_x64-setup.exe` | 2,220,490 | `2558425EB360C4B37F4CE2CA24DA40C788890C48D379E488B433C7BFA493AC72` |

The release operator ran the following test against the final executable. It passed in WebView2 at `tauri.localhost`.

```powershell
node tests/desktop.mjs
```

The test launches the executable with a fresh WebView2 profile and connects over CDP. It captures the cyan idle canvas before interaction, toggles immersive mode, exercises simulated listening, and submits messages through **Mensagem** and **Enviar**. A greeting transitions through thinking to responding. Submitting `alerta` transitions through thinking to alert. The test also confirms that graphics quality and reduced-motion preferences survive a reload. [Desktop idle screenshot](assistant-idle-desktop.png) · [Desktop alert screenshot](assistant-desktop.png)

The browser version of the interaction test runs against Vite with `node tests/scene.mjs`. [Browser alert screenshot](assistant.png)

## Earlier installer verification

An earlier 8.0.0 bundle, built with Tauri CLI 2.12.6, Rust 1.100.0 and the installed MSVC x64 tools, passed a silent install and uninstall. Setup installed `JARVIS.exe` in a fresh per-user directory and registered JARVIS 8.0.0 in Installed apps. Uninstall removed the executable and registration. That tested installer's SHA-256 was `68EE4932484B565FD9AAE3266CF9D304912685CF18AE273ABFEDA6EA8232C8B8`.

The final installer listed above has build and checksum verification. A fresh install/uninstall run for that exact checksum remains unverified. WebView2 was already present on the test machine, so provisioning it on a clean machine also remains unverified.
