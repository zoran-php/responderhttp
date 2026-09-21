; http_client/src-tauri/windows/hooks.nsh
;
; NSIS installer hooks (tauri.conf.json > bundle.windows.nsis.installerHooks).
;
; Uninstalling with "Delete the application data" ticked removes the data key
; from Credential Manager, so the key and the database it protects are always
; removed together (PLAN.md Phase 9, decided 2026-09-16). The two conditions
; are the ones Tauri's own template uses before it deletes $APPDATA\${BUNDLEID},
; which is where the database lives: the box is ticked, and this uninstall is
; not part of an update. An update must never remove the key.
;
; $DeleteAppDataCheckboxState and $UpdateMode belong to Tauri's installer.nsi
; template, not to a documented API. Re-check both names on every Tauri
; upgrade.
;
; cmdkey ships with Windows (System32). The app's own exe is already gone by
; the time this runs, so it cannot be asked to delete its key. The install is
; per-user, so this runs as the user who owns the credential. A failure only
; leaves the entry behind, which is the harmless direction, so the exit code
; is not checked.
;
; The target name must match KEYCHAIN_TARGET in src/secrets/keychain.rs; a
; test there reads this file to keep the two in step.

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    nsExec::Exec 'cmdkey /delete:data-key@io.github.zoran-php.responderhttp'
    Pop $0
  ${EndIf}
!macroend
