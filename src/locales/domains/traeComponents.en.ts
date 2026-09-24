/**
 * 文案域：**Trae 产品线 · 组件**（英文）。
 *
 * ⚠️ 不写类型注解：键名合法性由 `src/locales/en.ts` 单点校验。域文件保持零依赖。
 * 键必须与 `traeComponents.zh.ts` **一一对应**（缺哪条就回落中文）。
 */
export const en = {
  // =====================================================================
  // trae-types.ts —— program / variant display names
  // =====================================================================
  // Brand names are identical in both locales; they still go through the word list
  // so that adding a program triggers a compile-time reminder to add the key.
  "trae.program.traeWork": "TraeWork",
  "trae.program.traeCode": "TraeCode",
  "trae.program.traeWorkGlobal": "TraeWork AI",
  "trae.program.traeCodePending": "TraeCode (untested)",

  // =====================================================================
  // trae-account-card.tsx —— Trae account card
  // =====================================================================
  // ---- Duration / JWT status ----
  "trae.comp.card.hours.unknown": "Unknown",
  "trae.comp.card.hours.expired": "Expired",
  "trae.comp.card.hours.minutes": "{count} min",
  "trae.comp.card.hours.hours": "{count} h",
  "trae.comp.card.hours.days": "{count} d",
  "trae.comp.card.jwt.valid": "Valid {hours}",
  "trae.comp.card.jwt.warn": "Expiring {hours}",
  "trae.comp.card.jwt.expired": "Expired",
  "trae.comp.card.jwt.unparsable": "Unparsable",

  // ---- Expiry ----
  "trae.comp.card.expiry.permanent": "No expiry",
  "trae.comp.card.expiry.short": "Due {date}",
  "trae.comp.card.expiry.cooldownUntil": "Cooldown until {time}",
  "trae.comp.card.expiry.earliest": "Earliest expiry {time}",
  "trae.comp.card.expiry.none": "No expiry time yet",
  "trae.comp.card.expiry.noneShort": "No expiry",

  // ---- Body row labels ----
  "trae.comp.card.label.device": "Device",
  "trae.comp.card.label.jwtExpiry": "JWT expiry",
  "trae.comp.card.label.addedAt": "Added",
  "trae.comp.card.label.updatedAt": "Last updated",
  "trae.comp.card.label.group": "Group",

  // ---- Status chips ----
  "trae.comp.card.chip.checked": "Checked in",
  "trae.comp.card.chip.unchecked": "Not checked in",
  "trae.comp.card.chip.cooling": "Cooling down",
  "trae.comp.card.chip.jwtAuto": "Auto-refresh JWT",
  "trae.comp.card.chip.jwtManual": "JWT refreshable",
  "trae.comp.card.cooldown.until": " · until {time}",
  "trae.comp.card.cooldown.fallback": "Cooling down ({type})",

  // ---- Program switch controls ----
  "trae.comp.card.currentOf": "Current account in {label}",
  "trae.comp.card.switch.tip": "Switch to this account in {label} (restarts {label})",
  "trae.comp.card.switch.missing": "{label} not detected",
  "trae.comp.card.switch.aria": "Switch to {label}",
  "trae.comp.card.switch.ariaBusy": "Switching to {label}",
  "trae.comp.card.switch.busy": "Switching…",

  // ---- Action menu ----
  "trae.comp.card.manage.aria": "Manage account {name}",
  "trae.comp.card.manage.title": "More account actions",
  "trae.comp.card.menu.save": "Save login state",
  "trae.comp.card.menu.refreshJwt": "Refresh JWT",
  "trae.comp.card.menu.checkin": "Check in manually",
  "trae.comp.card.menu.thaw": "Clear cooldown",
  "trae.comp.card.menu.delete": "Delete account",

  // ---- Credits section ----
  "trae.comp.card.credits.remaining": "Credits left",
  "trae.comp.card.credits.unknown": "Credits not fetched",
  "trae.comp.card.credits.notQueried": "No credits fetched yet. Use “Refresh JWT” in the top-right menu to try again.",
  "trae.comp.card.section.expiring": "Expiring soon",
  "trae.comp.card.section.info": "Account info",
  "trae.comp.card.package.fallback": "Credit package",
  "trae.comp.card.package.tip": "{name} · {remaining} / {total} left · {expiry}",
  "trae.comp.card.package.remaining": "{credits} credits",
  "trae.comp.card.package.empty": "No credits available",

  // ---- Footer buttons ----
  "trae.comp.card.action.saving": "Saving…",
  "trae.comp.card.action.saveTip": "Back up the current Trae login state into this account slot",
  "trae.comp.card.action.refreshing": "Refreshing…",
  "trae.comp.card.action.refreshJwtTip": "Exchange the refresh token for a new JWT",
  "trae.comp.card.footer.updatedAt": "Updated {time}",
  "trae.comp.card.footer.noUpdate": "No update time recorded",
  "trae.comp.card.footer.updatedTip": "Last time this account was updated in the local account library",

  // ---- Details dialog ----
  "trae.comp.card.detail.open": "View account details",
  "trae.comp.card.detail.title": "Account details",
  "trae.comp.card.detail.subtitle": "{name} · UID {uid}",
  "trae.comp.card.detail.accountId": "Account ID",
  "trae.comp.card.detail.enabledPrograms": "Enabled programs",
  "trae.comp.card.detail.creditsExpiry": "Credits expiry",
  "trae.comp.card.detail.jwtStatus": "JWT status",
  "trae.comp.card.detail.jwtAutoRefresh": "Auto-refresh JWT",
  "trae.comp.card.detail.deviceId": "Device ID",
  "trae.comp.card.detail.cooldown": "Cooldown",
  "trae.comp.card.detail.ungrouped": "Ungrouped",
  "trae.comp.card.detail.none": "None",
  "trae.comp.card.detail.notQueried": "Not fetched",
  "trae.comp.card.detail.jwtAutoOn": "Enabled",
  "trae.comp.card.detail.jwtAutoOff": "Manual refresh only",
  "trae.comp.card.detail.noRefreshToken": "No refresh token",

  // =====================================================================
  // trae-oauth-login-dialog.tsx —— OAuth web login
  // =====================================================================
  "trae.comp.oauth.title": "OAuth web login · {variant}",
  "trae.comp.oauth.desc.lead": "Sign in to",
  "trae.comp.oauth.desc.mid": "in your browser and authorize. The app catches the callback automatically and adds the account to",
  "trae.comp.oauth.desc.tail": "'s account library — no token pasting needed.",
  "trae.comp.oauth.error.fallback": "Sign-in failed",
  "trae.comp.oauth.error.timeout": "Timed out waiting for authorization ({seconds}s). Confirm that you finished authorizing in the browser; if you did and it still timed out, the authorization page usually failed to send the callback back to the local listening port (see the port below). Close this dialog and try again; if it keeps timing out, make sure port 17388 on this machine is not taken by another program. You can also use “Import local accounts” instead.",
  "trae.comp.oauth.start": "Start {variant} web sign-in",
  "trae.comp.oauth.startBusy": "Starting sign-in for {variant}…",
  "trae.comp.oauth.waiting": "Waiting for authorization; please finish signing in in your browser…",
  "trae.comp.oauth.remaining": "{time} left",
  "trae.comp.oauth.callback": "Local callback listener: ",
  "trae.comp.oauth.result": "Account added: {name}",
  "trae.comp.oauth.close": "Close",
  "trae.comp.oauth.done": "Done",
  "trae.comp.oauth.retry": "Restart sign-in",
  "trae.comp.oauth.retryBusy": "Starting sign-in…",
  "trae.comp.oauth.launch": "Launch the {variant} client",
  "trae.comp.oauth.launchBusy": "Launching the {variant} client…",
  "trae.comp.oauth.launchOk": "The {variant} client has been launched. Wait for it to write its device credential (a first launch also shows a sign-in page — sign in there once), then click “Restart sign-in”.",

  // =====================================================================
  // trae-import-accounts-dialog.tsx —— Import accounts
  // =====================================================================
  "trae.comp.import.title": "Import accounts",
  "trae.comp.import.desc": "Choose a Trae account backup JSON, then select the accounts to import.",
  "trae.comp.import.uid": "UID · {uid}",
  "trae.comp.import.item": "Item {index}",
  "trae.comp.import.chooseFile": "Choose file",
  "trae.comp.import.parsing": "Parsing…",
  "trae.comp.import.summary": "{count} of {total} accounts selected",
  "trae.comp.import.selectAll": "Select all",
  "trae.comp.import.deselectAll": "Deselect all",
  "trae.comp.import.missingJwt": "Missing JWT",
  "trae.comp.import.cancel": "Cancel",
  "trae.comp.import.busy": "Importing…",
  "trae.comp.import.submit": "Import selected",

  // =====================================================================
  // trae-export-accounts-dialog.tsx —— Export accounts
  // =====================================================================
  "trae.comp.export.title": "Export accounts",
  "trae.comp.export.desc": "Select the Trae accounts to export into a JSON file.",
  "trae.comp.export.uid": "UID · {uid}",
  "trae.comp.export.reveal.windows": "Show in File Explorer",
  "trae.comp.export.reveal.linux": "Show in file manager",
  "trae.comp.export.reveal.mac": "Show in Finder",
  "trae.comp.export.saveTitle": "Export Trae accounts",
  "trae.comp.export.warningTitle": "Security notice",
  "trae.comp.export.warningBody": "The exported file contains a Cloud-IDE-JWT, which is equivalent to a password. Do not upload it to cloud storage or send it to anyone.",
  "trae.comp.export.empty": "There are no accounts to export.",
  "trae.comp.export.summary": "{count} of {total} accounts selected",
  "trae.comp.export.selectAll": "Select all",
  "trae.comp.export.deselectAll": "Deselect all",
  "trae.comp.export.successTitle": "Export complete",
  "trae.comp.export.done": "Done",
  "trae.comp.export.cancel": "Cancel",
  "trae.comp.export.submit": "Export selected",
};
