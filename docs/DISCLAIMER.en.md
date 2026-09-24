# Disclaimer

By using this project you acknowledge that you have read, understood and agreed to all of the terms below. If you do not agree, stop using it and uninstall it immediately.

[简体中文](DISCLAIMER.md) · **English**

> This page is the full version of the disclaimer in the README and is the **authoritative** one: where the warning block at the top of the README or the wording in [Licensing](LICENSING.en.md) differs from this page, this page governs.

## 1. Unofficial third-party tool

- This project is a community-maintained **source-available tool** released under a noncommercial license. It is **not** an official product of WorkBuddy, CodeBuddy, CodeBuddy CN, Trae, TraeWork or any of their affiliates, and it has no affiliation, partnership, authorization, sponsorship or endorsement relationship with any of them.
- All product names, trademarks, service marks and logos appearing here are the property of their respective owners. They are used only to identify the clients this tool works with, and this project claims no rights over them.
- This project does not contain, embed or redistribute any source code, binaries or private assets of those third-party clients. It only reads and writes configuration and data files that those clients already keep on your machine.

## 2. Terms of service and compliance are your responsibility

- Managing multiple accounts, switching login state, batch or scheduled check-in, and forwarding quota **may not comply with** the terms of service, subscription agreements, employer policies or laws of your jurisdiction that apply to you.
- Whether your usage is compliant is **yours to determine**. Any account restriction, quota clawback, ban, breach claim or legal dispute arising from the use of this project is borne entirely by you; the author accepts no liability.
- Do not use this tool for any purpose that violates applicable laws, regulations or terms of service.

## 3. Data writes and backups

- Switching accounts, cloning sessions, migrating long-term memory and connector configuration, and writing Trae login state all **directly modify data files of third-party clients** on your machine (see [Data and privacy](../README.en.md#data-and-privacy)).
- The project attempts to back up data before rewriting it, but a backup can fail for reasons such as an unwritable path, insufficient disk space or the target file being locked. **The automatic backup is not a reliability guarantee of any kind.**
- **Keep your own independent backups of anything important.** The author accepts no liability for any data loss, corruption, inconsistency or abnormal login state.

## 4. Risks of automated behaviour

- Automatic check-in, Cat Travel, token keep-alive, automatic rotation and scheduled tasks **send requests to the official services on the schedule you configure**.
- The targets, frequency and timing of those requests are entirely determined by you. Any risk-control decision, rate limiting, CAPTCHA, human-verification challenge or other platform-side measure triggered by that traffic is your responsibility.

## 5. API gateway exposure risks

- The gateway binds to `127.0.0.1` by default. Once you switch it to `0.0.0.0`, or expose it through a reverse proxy, tunnel or port mapping, **anyone who can reach that address may consume your model quota and read the responses**.
- A gateway API key is returned in plaintext once, at creation time; the server stores only its prefix and hash. **If a key leaks, revoke it in the UI immediately.**
- Whether to expose the gateway, and which network and authentication protections to apply, is entirely your responsibility.

## 6. Provided "as is"

- This project is provided "as is" under the [PolyForm Noncommercial License 1.0.0](../LICENSE), **without warranty of any kind, express or implied**, including but not limited to the implied warranties of merchantability, fitness for a particular purpose and non-infringement. For a plain-language summary of the license, see [Licensing](LICENSING.en.md).
- This project depends on the internal data structures and endpoints of third-party clients, which may change in any release and render part or all of this tool non-functional. **No compatibility with any specific client version is guaranteed.**
- Regardless of the theory of liability — contract, tort (including negligence) or otherwise — the author is not liable for any direct, indirect, incidental, special or punitive loss arising from the use of, or inability to use, this project.
