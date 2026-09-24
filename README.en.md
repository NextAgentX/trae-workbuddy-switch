<p align="center">
  <img src="public/icon-transparent.png" alt="Buddy Switch icon" width="128" />
</p>

<p align="center">
  <strong>Buddy Switch</strong><br />
  Account switcher for WorkBuddy / CodeBuddy / Trae
</p>

<p align="center">
  <a href="README.md">简体中文</a> · <strong>English</strong>
</p>

# Buddy Switch

**A multi-account manager for WorkBuddy / TraeWork**: OAuth QR-code sign-in, one-click login-state switching, credit-expiry monitoring with automatic check-in, token usage statistics — and it can expose your model quota to other local tools through an OpenAI / Anthropic compatible endpoint.

> ⚠️ **Disclaimer**: this project is an **unofficial, third-party tool released under a noncommercial license** (source-available). It is not affiliated with, authorized by, sponsored by or endorsed by WorkBuddy, CodeBuddy, Trae / TraeWork or their respective rights holders. It reads and writes authentication data of third-party clients on your machine, sends automated requests on the schedule you configure, and can forward your model quota to other tools through a local endpoint. Before using it, read the [Disclaimer](docs/DISCLAIMER.en.md) in full, assess the risks yourself and make sure your usage complies with the applicable terms of service.

One interface, three forms:

| Form | How to get it | Notes |
| --- | --- | --- |
| **Desktop app** | Download the installer from [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest) | Packaged with Tauri; recommended for daily use |
| **webui (browser)** | Build from source, see [webui form](#webui-form-build-from-source) | The same frontend bundle as the desktop app, served over a local HTTP channel |
| **Online demo** | [GitHub Pages](https://nextagentx.github.io/trae-workbuddy-switch/) | Read-only demo; accounts, credits and request records are fictional data and all business actions are disabled |

> **No npm distribution yet**: the npm package `@nextagentx/buddy-switch` has not been published, so `npm i -g …` will not install it yet.
> For the webui form, follow [Build from source](#webui-form-build-from-source) below; the install command will be added here once it is published.

## Two product sections

Switch product sections at the top of the sidebar. The two sections are **fully independent**: separate account stores, separate clients and separate configuration, with no effect on each other.

| Section | Clients managed | Regions |
| --- | --- | --- |
| **WorkBuddy** | WorkBuddy desktop client, CodeBuddy CLI, CodeBuddy CN IDE | CN (WorkBuddy) / Global (WorkBuddy AI) |
| **TraeWork** | Trae Work (clients `TRAE SOLO CN` / `TRAE SOLO`), Trae IDE (`Trae CN` / `Trae`) | CN / Global |

The pages under both sections are **structurally identical, item for item**: Accounts, Token Stats, Credit Stats, API Service, Settings.

> **Regions and client variants on the Trae side**: Trae has two product lines that can coexist on one machine (Trae Work and Trae IDE; the clients self-identify as `TraeWork CN` / `TraeCode CN`). They are switched **inside** the Trae section and do not occupy the sidebar. The variant is carried by the `?line=` URL parameter, so refreshing, sharing links and going back/forward all keep your current position.

## Quick start

### Desktop app

Download the installer for your platform from [GitHub Releases](https://github.com/NextAgentX/trae-workbuddy-switch/releases/latest):

| Platform | Installer | Installation |
| --- | --- | --- |
| macOS Apple Silicon (M-series, arm64) | `BuddySwitch_<version>_aarch64.dmg` | Open the DMG and drag `BuddySwitch.app` into Applications |
| macOS Intel (x86_64) | `BuddySwitch_<version>_x86_64.dmg` | Open the DMG and drag `BuddySwitch.app` into Applications |
| Windows x64 | `BuddySwitch_<version>_x64-setup.exe` | Run the installer and follow the prompts |
| Linux x64 | `BuddySwitch_<version>_amd64.deb` / `BuddySwitch_<version>_amd64.AppImage` | Install the `.deb` on Debian/Ubuntu; on other distributions mark the AppImage executable and run it directly |

On macOS, if the first launch reports that the developer cannot be verified, Control-click the app in Finder and choose **Open**, or go to **System Settings → Privacy & Security** and choose **Open Anyway**. Only when the installer came from the official Releases above and the system still reports it as damaged, run:

```bash
xattr -rd com.apple.quarantine "/Applications/BuddySwitch.app"
```

If the app starts but reports a permission error when switching accounts, see [macOS permissions](#macos-permissions) below.

### webui form (build from source)

This project is not distributed through npm yet, so the webui form has to be compiled locally. **Mind the order**: the frontend bundle is embedded into the server binary at **compile time** (`rust-embed`), so you must build the frontend first and the server second:

```bash
npm install
npm run build                                    # 1) build the frontend → dist/
cargo build --release -p buddy-switch-server     # 2) build the server (embeds dist/)
./target/release/buddy-switch                    # 3) start the local service + open the browser (buddy-switch.exe on Windows)
```

Subcommands available after building:

```bash
buddy-switch              # start the local service and open the browser
buddy-switch serve        # start the service only, without opening a browser (--port to set the port)
buddy-switch status       # print the current account in the terminal
buddy-switch version      # print the version
```

The webui listens on `127.0.0.1:57890` by default. The interface is identical to the desktop app — the same frontend bundle, with the desktop app going through the Tauri command channel and the webui through a local HTTP channel.

> If you change the frontend without rebuilding the server, the UI will not change (the embedded bundle is stale). Stop any running `buddy-switch` before rebuilding, otherwise linking fails on Windows because the binary is locked.

## Features

### WorkBuddy section

| Module | Description |
| --- | --- |
| Accounts | OAuth QR-code sign-in, import from this machine, import from a backup file, add a token manually, delete accounts, export account backups |
| Account switching | Back up the auth file → close WorkBuddy → write the target account → restart, with live progress feedback throughout the switch |
| Session cloning | Copy the sessions selected on the current account to the target account under new ids (jsonl content + `workbuddy.db` index + edge-sync registration) |
| Automatic check-in | Enabled by default; checks on startup and catches up on a schedule; supports check-in all and a check-in log |
| Automatic travel | Automatically dispatches and claims the "Cat Travel" task |
| Token keep-alive | Lazy refresh (refresh only when the remaining lifetime is below a threshold before an operation) plus a daily keep-alive (one unconditional refresh per day by default), preventing refresh-token expiry |
| Credit expiry lookup | Query every account's credit grants, remaining amounts and expiry times; grants expiring within 7 days are highlighted and sorted by urgency, with the most urgent marked "use first" |
| Credit statistics | Aggregate official WorkBuddy request usage with daily trends, model breakdown, per-account consumption and request details; when official data is unavailable it clearly falls back to observing local balance snapshots |
| Token statistics | Separate overviews for WorkBuddy, CodeBuddy CLI and CodeBuddy IDE; input, output and cache read/write shown in K/M/B, with composition ratios, an activity heatmap, project/model top 10 and session ranking |
| CodeBuddy CLI | Reuses the same account store as WorkBuddy but keeps a separate default account; on macOS/Linux through `apiKeyHelper`, on Windows through `env.CODEBUDDY_AUTH_TOKEN` in `settings.json` for subsequent sessions |
| CodeBuddy CN IDE | Reuses the same account store; injects Safe Storage credentials into the `CodeBuddy CN` desktop client and restarts the IDE |
| Automatic rotation | A background job periodically sets the CodeBuddy CLI's account for subsequent launches to the account with the most urgent credit expiry; the current session keeps its original account |
| Scheduled tasks | Six task types — check-in, Cat Travel, Activity Map, token keep-alive, Back-to-School and Night Owl — each with its own toggle and configurable execution hour |
| Migrate data on switch | Merge the current account's long-term memory and connector configuration into the target account (same-name entries merged recursively, deduplicated by content), with same-version and cross-version migration supported; the original content is backed up before being rewritten |
| Permission check | macOS authorization walkthrough (App Management / Full Disk Access drag-to-authorize plus automatic detection) |
| Auto-update | Check GitHub Releases for new versions; full-package updates are signature-verified (tauri-updater) |

### TraeWork section

| Module | Description |
| --- | --- |
| Accounts | OAuth web sign-in (browser callback), paste `Cloud-IDE-JWT` manually, import from this machine, import from a backup file, delete accounts, export account backups |
| Groups | Accounts can be grouped, and the list can be filtered by group |
| Check-in | One-click check-in with credit refresh; supports "skip accounts already checked in today" and automatic renewal; account cards show check-in status and cooldown |
| Credits | Per-account remaining credits, per-package breakdown and expiry times; 7-day totals / earned / consumed trends |
| Token statistics | Token and call-count statistics over 7-day / 30-day / 90-day / all-history windows |
| Login-state switching | Write the selected account into the Trae client's login state (`profiles/`), with an automatic backup before switching; takes effect after restarting the client |
| Login-state snapshots | Save / restore / delete client login-state snapshots for easy rollback |
| JWT refresh | Refresh an account's JWT manually and see the remaining validity period (highlighted when close to expiry) |
| Device identifier | View and reset the device identifier |
| Environment detection | Automatically detect the installed product lines, client versions, running state and userData directories |
| Runtime logs | Client data directories, gateway request logs and operation logs viewed in one place |

### API gateway (independent per section)

Expose the current account's model quota to other local AI tools (Cursor, Claude Code, OpenWebUI, …) through **compatible endpoints**. Keys and account pools are isolated per product section and per region.

| Capability | Description |
| --- | --- |
| Compatible protocols | OpenAI-compatible (`/v1/models`, `/v1/chat/completions`) and Anthropic-compatible (`/v1/messages`), both supporting SSE streaming |
| Listen configuration | Binds to `127.0.0.1` only by default; changing it to `0.0.0.0` requires an explicit second risk confirmation |
| Default ports | WorkBuddy gateway `57891`; Trae gateway `7864` (Trae also has a local MITM proxy port `8899`, which login-state capture depends on) |
| API keys | Multiple keys can be created, each belonging to a specific region; the list shows only the prefix and **never stores plaintext or echoes the hash**; keys can be revoked and deleted |
| Model catalog | Fetch and display the available model list, with manual refresh |
| Account policy | Configure the account-selection strategy and the account-pool balance refresh interval |
| Request log | Record the model, account, tokens and status of every request; can be cleared |

## Usage

### WorkBuddy

1. **Add an account**: Accounts page → "Add via OAuth QR" (device flow), "Import local account" or "Import backup"
   - Since client 5.6, the login-state tokens are stored encrypted. **"Import local account" automatically calls the local client to unwrap that encryption** (requires the WorkBuddy desktop app to be installed), so check-in / credits / token stats work right after importing. If the client is missing or has changed its encryption scheme, the account is **still imported** but can only be used for "Switch"; click "Refresh token" on the account to retry unwrapping
2. **Switch accounts**: account card → "Switch"; you can optionally copy the current account's sessions along and merge long-term memory and connector configuration into the target account (for cross-version migration you can choose the data source region)
3. **Automatic check-in / Cat Travel**: toggle them directly at the top of the Accounts page; the Settings page lets you tune parameters, run a check-in immediately and view logs
4. **Check credit expiry**: the Accounts page automatically queries each account's credit grants; click "Refresh credits" to update manually — grants approaching expiry are highlighted and sorted by urgency
5. **View credit statistics**: sidebar → "Credit Stats" — overview, 30-day trend, model breakdown, per-account consumption and request details; changing the account or date filter does not re-hit the official API, only "Refresh statistics" collects again
6. **View token statistics**: sidebar → "Token Stats"; choose WorkBuddy, CodeBuddy CLI or CodeBuddy IDE to see input, output, cache read/write and call counts
7. **Connect CodeBuddy CLI**: one-click connect / update authentication on the Accounts page. "Switch CodeBuddy" only updates the default account used by **sessions loaded afterwards**; the currently running session is not switched — reload the session from ACP, or restart CodeBuddy CLI for it to take effect
8. **Switch CodeBuddy CN IDE**: switch the CN desktop client (www.codebuddy.cn) with one click from an account card. Switching closes and restarts CodeBuddy CN; before the first use, open and sign in to it manually once so that the Keychain Safe Storage entry is created
9. **Automatic rotation**: Settings → CodeBuddy CLI auto-rotation (policy below)
10. **Updates**: the app automatically checks the public GitHub Releases and lets you upgrade directly from the bottom-left corner

### TraeWork

1. **Add an account**: Accounts page → "OAuth web sign-in" (the browser completes authorization and then calls back) or "Paste JWT"; you can also use "Import local account" / "Import backup"
2. **Choose region and product line**: switch CN / Global at the top of the page; on an account card, switch the client variant to operate (TraeWork / TraeCode)
3. **Check-in**: "Check in and refresh credits" on the Accounts page, or per account; enable "Skip already checked in" to avoid duplicate requests
4. **Switch login state**: account card → "Switch", which writes the selected client's login state and restarts the client; a snapshot is saved automatically before switching
5. **Snapshot rollback**: Settings → "Login-state snapshots" to save / restore / delete
6. **API service**: sidebar → "API Service"; create a key and follow the integration guide on the page to configure it in your tool

### Automatic rotation policy (WorkBuddy)

The goal of automatic rotation is to avoid wasting credits by letting them expire: a background job periodically checks every account's credit expiry and sets the CodeBuddy CLI's default account for subsequent sessions to the **most urgent** one (earliest expiry that still has credits remaining). Neither the `apiKeyHelper` on macOS/Linux nor the settings env on Windows replaces the token a running session already holds; the rotation takes effect after ACP reloads the session or the CLI restarts. To avoid the default account changing too often, every check decides in this order:

1. **Eligible accounts**: only accounts that queried successfully, are not expired and still have credits remaining can be selected as the target
2. **Urgency check**: if every account expires far in the future (the most urgent one has more than `min_urgency_hours` left, default 72 hours) → do not switch
3. **Already the target**: if the CLI's default account already is the most urgent one → do not switch
4. **Cooldown**: do not switch again within `cooldown_minutes` (default 120) of the last switch
5. **Activity guard**: if a CLI session wrote within the last `active_guard_minutes` (default 30), i.e. a conversation is in progress → do not switch
6. **Value filter**: if the target account's remaining credits are below `min_remaining_credits` it is not worth switching (default 0, disabled; every check logs each account's remaining credits so you can tune this)
7. **Debounce**: if the target expires earlier than the current account but by less than `min_gap_hours` (default 24) → do not switch

> **Scope**: automatic rotation only updates the default account used by subsequent restore/load operations; it does not hot-switch the current session. On macOS/Linux the next helper run reads the latest account; on Windows the latest token is written into settings. A running session keeps the account it obtained at startup or load time — reload the session from ACP, or restart CodeBuddy CLI.

Configuration keys: `check_interval_minutes` (check interval, default 5), `cooldown_minutes`, `min_urgency_hours`, `active_guard_minutes`, `min_remaining_credits`, `min_gap_hours`. Adjust them on the Settings page, or edit `~/.buddy-switch/auto_rotate_config.json` directly.

## Screenshots

### WorkBuddy accounts

Account cards show login state, check-in state, credit balance and near-expiry grants in one place; the region switcher at the top toggles between WorkBuddy CN and Global, whose account stores are isolated from each other.

<table>
  <thead>
    <tr>
      <th>Light mode</th>
      <th>Dark mode</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/workbuddy-accounts-light.png" alt="WorkBuddy Accounts page (light mode, demo data)" /></td>
      <td><img src="docs/images/workbuddy-accounts-dark.png" alt="WorkBuddy Accounts page (dark mode, demo data)" /></td>
    </tr>
  </tbody>
</table>

### Credit statistics

Shows official request usage, daily trends, model breakdown and per-account consumption, and clearly labels the data source and update time; the top of the page switches between CN, Global and a merged view.

<table>
  <thead>
    <tr>
      <th>Light mode</th>
      <th>Dark mode</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/credit-stats-light.png" alt="Credit Stats page (light mode, demo data)" /></td>
      <td><img src="docs/images/credit-stats-dark.png" alt="Credit Stats page (dark mode, demo data)" /></td>
    </tr>
  </tbody>
</table>

### TraeWork accounts

Switch CN / Global at the top; the status bar shows client version, running state, today's check-in, available credits and JWT expiry status; account cards offer client-variant switching, check-in, login-state switching and cooldown clearing.

<table>
  <thead>
    <tr>
      <th>Light mode</th>
      <th>Dark mode</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/trae-accounts-light.png" alt="TraeWork Accounts page (light mode, demo data)" /></td>
      <td><img src="docs/images/trae-accounts-dark.png" alt="TraeWork Accounts page (dark mode, demo data)" /></td>
    </tr>
  </tbody>
</table>

### API service

Base URLs and representative keys are listed per region, and the key list supports creation, revocation and deletion; below it you will find the model catalog, account policy, integration guide and request log.

<table>
  <thead>
    <tr>
      <th>Light mode</th>
      <th>Dark mode</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/api-service-light.png" alt="API Service page (light mode, demo data)" /></td>
      <td><img src="docs/images/api-service-dark.png" alt="API Service page (dark mode, demo data)" /></td>
    </tr>
  </tbody>
</table>

> The screenshots above are taken from a demo build; accounts, credits and request records are all fictional data.

## Data and privacy

- All data is stored locally under `~/.buddy-switch/` and is never uploaded to any third-party service
- Account stores: WorkBuddy uses `accounts.json` (CN) / `accounts.global.json` (Global); Trae uses `trae/checkin_accounts.json` (CN) / `trae/checkin_accounts.global.json` (Global)
- Auth files: WorkBuddy uses `workbuddy-desktop.info` / `workbuddy-desktop-ai.info`
- The original auth file is backed up before switching accounts; Trae saves a snapshot before switching login state
- Gateway API keys are stored as prefix and hash only; the plaintext is returned once at creation time
- Local data is never committed to the repository; a token-pattern scan (`ghp_` / `npm_` / `gho_` etc.) runs before release

## macOS permissions

Switching accounts writes to WorkBuddy's auth file, which macOS gates behind **App Management** (or **Full Disk Access**) authorization:

1. If the first switch reports "no permission", click "Open System Settings"
2. Prefer enabling the BuddySwitch switch under **App Management**; if it is not listed there, go to **Full Disk Access** and drag BuddySwitch into the list
3. Restart this app for the authorization to take effect; "Permission check" on the Settings page lets you verify it at any time

> webui mode: permissions are those of the terminal process that started the service; if that terminal already has Full Disk Access, no extra steps are needed.

## Development

Environment requirements, build commands, release process and directory layout are documented in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). In brief:

```bash
npm install
npm run tauri dev     # desktop development mode
npm run build         # build the frontend (dist/)
npm run check:api     # verify the frontend API contract matches the backend
```

The repository is a Rust workspace plus a Vite/React frontend:

```
crates/
  buddy-switch-core/     # core logic: accounts, auth, OAuth, processes, switching, sessions, check-in, refresh, regions and the Trae module
  buddy-switch-gateway/  # OpenAI / Anthropic compatible gateway (one implementation each for WorkBuddy and Trae)
  buddy-switch-server/   # HTTP server + CLI (axum API + rust-embed embedded frontend)
src-tauri/               # desktop host (thin Tauri command wrappers + tray)
src/                     # frontend: components / pages / lib (api.ts dual channel: Tauri invoke or HTTP fetch)
npm/                     # npm packages (**not published yet**): main package @nextagentx/buddy-switch + 5 platform packages
```

## Contributing

**Contributions of every kind are welcome — pull requests included.** Issues, documentation, bug fixes and features all count — see **[CONTRIBUTING.en.md](CONTRIBUTING.en.md)** ([简体中文](CONTRIBUTING.md)) for the full guide: what you can work on, the PR checklist and this repository's code conventions.

## Support the project

If Buddy Switch has been useful to you, you can buy the author a drink ☕

<table>
  <thead>
    <tr>
      <th>WeChat Pay</th>
      <th>Alipay</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><img src="docs/images/donate-wechat.png" alt="WeChat Pay QR code" width="260" /></td>
      <td><img src="docs/images/donate-alipay.jpg" alt="Alipay QR code" width="260" /></td>
    </tr>
  </tbody>
</table>


## Acknowledgements

This project drew on the following open-source projects during its development. Thanks to their authors:

- [changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch) — for the WorkBuddy account data format and the general approach to multi-account switching.
- [Sliverkiss/workbuddy2api](https://github.com/Sliverkiss/workbuddy2api) — for the idea of exposing WorkBuddy model quota as an OpenAI-compatible API service.

Each of those projects is governed by its own license. This project is an independent implementation with no affiliation, partnership or endorsement relationship with any of them, and all names and trademarks belong to their respective owners. This project has its own codebase, release channel and data directory, and does not replace, overwrite or rewrite any other project's update source or data directory.

## Disclaimer

By using this project you acknowledge that you have read, understood and agreed to all of the terms below. If you do not agree, stop using it and uninstall it immediately.

The full text lives in **[docs/DISCLAIMER.en.md](docs/DISCLAIMER.en.md)** ([简体中文](docs/DISCLAIMER.md)) and covers six sections: unofficial third-party tool; terms of service and compliance; data writes and backups; risks of automated behaviour; API gateway exposure risks; provided "as is".

## License

This project is licensed under the **[PolyForm Noncommercial License 1.0.0](./LICENSE)**: **noncommercial use is permitted, commercial use is not licensed** (commercial use requires prior written authorization from the author).

- Full text: [LICENSE](./LICENSE)
- Plain-language summary, what is permitted and what is not, FAQ, and the MIT note for versions before 2026-09-22: [docs/LICENSING.en.md](docs/LICENSING.en.md)
- For commercial licensing, please contact the author via this repository's Issues.
