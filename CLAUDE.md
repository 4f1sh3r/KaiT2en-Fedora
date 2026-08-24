# Projekt-Workflow: T2 Kernel-Module (Fedora)

## Über dieses Repo

Dieses Repo enthält mehrere unabhängige T2-Mac-Kernel-Module und zugehörige
Fedora-Integration (u.a. `t2bce_core`/`t2bce_audio`/`t2bce_dma`/`t2bce_vhci`,
`t2touchbar`, `t2bdrm`, `t2gmux`, `t2smc`, `t2mfi_fastcharge`,
`hid_t2magicmouse`). Je nach Auftrag kümmern wir uns um unterschiedliche
Module und deren jeweils eigene Probleme (Audio-Xruns, Touch Bar, Trackpad,
Suspend/Resume, Grafik-Mux, SMC, Laden, Maus-HID, ...) – nicht jede Änderung
betrifft t2bce oder Suspend/Resume.

## Branch-Strategie

- `main` bleibt immer clean und unangetastet für eigene Commits. Auf `main`
  wird nur regelmäßig der neueste Upstream-Stand gepullt (`git pull origin
  main`) – niemals direkt auf `main` committen oder daran herumbauen.
- Für jedes spezifische Problem (z.B. Audio-Xruns, Touch Bar, Trackpad,
  Suspend/Resume, ...) wird ein eigener Branch von `main` abgezweigt. Vor dem
  Abzweigen sicherstellen, dass `main` aktuell ist (siehe oben).
- Beim Anlegen eines neuen Themen-Branches gleich eine eigene, themenbezogene
  Learnings-Datei dafür anlegen (z.B. `T2BCE_AUDIO_LEARNINGS.md` für einen
  Audio-Branch), analog zu den bestehenden `LEARNINGS.md`/
  `T2BCE-LEARNINGS.md`.
- Auf einem Themen-Branch darf ganz normal nach jeder Änderung committed
  werden – anders als bei uncommitteten Experimenten (siehe unten), gibt es
  hier keinen Grund, mit dem Commit zu warten, da der Branch ohnehin isoliert
  von `main` ist.
- Hinweis nur falls doch mal von einer separaten Entwicklungsmaschine aus
  gearbeitet wird (Normalfall ist, direkt auf dem T2 Mac zu arbeiten, siehe
  Entwicklungs-Setup): dann müssten lokales Arbeitsverzeichnis und Mac immer
  auf demselben Branch stehen, da `sync-to-mac.sh` nur Dateien überträgt und
  dabei kein Git kennt (`.git/` wird nicht mitgesynct). `sync-to-mac.sh`
  prüft das vor jedem Sync und checkt auf dem Mac automatisch den passenden
  Branch aus; bei Unstimmigkeiten, die es nicht automatisch auflösen kann,
  bricht es ab statt falsch zusammenzuführen.

## Integrations-Branch `local/rig`

- `local/rig` ist der einzige Branch, aus dem auf diesem Mac installiert
  wird. Er ist `main` plus Merges aller Themen-Branches, die hier gerade im
  Test sind. Er wird niemals zu einem PR und wird niemals zurück nach `main`
  gemerged.
- Neue Features/Branches kommen NUR nach ausdrücklicher Rückfrage beim
  Nutzer in `local/rig` – nie automatisch, auch nicht, wenn es naheliegend
  erscheint.
- `RIG.md` (nur auf `local/rig`) hält fest, was gerade gemerged ist, woher es
  kommt, warum es getestet wird und warum es noch nicht upstream ist. Bei
  jeder Änderung an den gemergten Branches muss `RIG.md` mitgepflegt werden;
  landet etwas upstream oder wird verworfen, wird es dort entfernt und
  wieder ausgemerged.
- Update-Zyklus: `main` aktualisieren (siehe Branch-Strategie), dann `git
  merge main` in `local/rig`. `rerere.enabled` ist gesetzt, damit
  wiederkehrende Merge-Konflikte nur einmal gelöst werden müssen.
- App- und Modul-Installation sind unabhängig:
  `scripts/fedora/install-apps.sh --react-drm-only` fasst den DKMS-Stack
  nicht an. Für die Kernel-Module gilt `install-dkms-modules.sh` – und beides
  ausschließlich mit ausgechecktem `local/rig`, da diese Skripte immer den
  gerade ausgecheckten Baum bauen.

## Entwicklungs-Setup

- Claude Code läuft direkt auf dem T2 Mac selbst (Hostname
  `afis-fedora-mac`) – der T2 Mac IST die aktuelle Arbeitsumgebung, nicht nur
  ein per SSH erreichbares Testgerät. Code-Änderungen werden also direkt
  lokal im aktuellen Shell-Prozess gebaut, geladen und mit Debug-Logging
  ausprobiert; kein `ssh mac-t2` und kein `./sync-to-mac.sh` nötig.
- Das Repo liegt unter `/home/alex/Git/priv/kait2en/KaiT2en-Fedora`.
- Der SSH-Host-Alias `mac-t2` und `./sync-to-mac.sh` existieren weiterhin für
  den Fall, dass von einer separaten Entwicklungsmaschine aus auf den Mac
  synchronisiert werden muss – für den normalen Workflow direkt auf dem Mac
  sind sie nicht relevant.

## Nach jeder Code-Änderung

1. Direkt lokal (kein `sync-to-mac.sh`, kein SSH nötig) je nach betroffenem
   Modul bauen und laden – der richtige Weg hängt vom Modul/der Änderung ab,
   z.B.:
   - `sudo bash scripts/fedora/install-dkms-modules.sh` (alle DKMS-Module neu
     bauen/installieren), oder gezielt `dkms install <modul>/<version>`
   - Modul per `rmmod`/`insmod`/`modprobe` neu laden, wenn das für einen
     schnellen Testzyklus reicht und kein Reboot nötig ist
   - Reboot nur, wenn die Änderung das erfordert (z.B. ACPI/DSDT, Kernel-Args,
     oder Verhalten, das nur über einen echten Suspend/Resume- bzw.
     Boot-Zyklus sichtbar wird)
2. Vor dem Test durch den Nutzer prüfen, dass wirklich die frisch gebaute
   Version läuft:
   - `modinfo <modul> | grep -E "srcversion|vermagic"` (installiertes `.ko`)
   - `cat /sys/module/<modul>/srcversion` (tatsächlich geladenes Modul)
   - `uname -r` (laufender Kernel, muss zum vermagic passen)
   - srcversion von installiertem und geladenem Modul müssen übereinstimmen.
     Bei Abweichung: NICHT den Nutzer testen lassen, sondern Ursache klären
     (z.B. Reboot zu früh/fehlend, initramfs stale, DKMS-Build fehlgeschlagen).
3. Dem Nutzer mitteilen, was konkret zu testen ist (abhängig vom geänderten
   Modul, z.B. Touch Bar, Audio, Trackpad/Maus, Suspend/Resume, Grafik-Mux,
   Laden) – und dann auf die Rückmeldung warten, bevor die nächste Änderung
   gemacht wird.
4. Auf einem Themen-Branch (siehe Branch-Strategie) darf sofort mit `git add`
   + `git commit -m "<aussagekräftige Beschreibung>"` committed werden;
   uncommittete Zwischenstände dürfen aber genauso beliebig oft direkt lokal
   getestet werden, bevor committed wird – beides ist in Ordnung. Auf `main`
   wird grundsätzlich nicht committed.

## Wichtig

- Bei Build-Fehlern auf dem Mac: Fehlermeldung analysieren, NICHT
  automatisch reboot durchführen, sondern erst den Fehler beheben.
- Reboot nur nach erfolgreichem Build, und nur wenn für den Test tatsächlich
  nötig (siehe oben).
- Für das stateful Suspend/Resume-fähige `t2bce`/`apple-bce`-Derivat in
  diesem Repo gilt: es geht über den Stand hinaus, der in der
  t2linux-Community/im öffentlichen Internet dokumentiert ist. Für die
  verbleibenden Bugs dort gibt es daher KEINE externen Quellen/Fixes zu
  finden – nicht im Internet danach suchen, sondern die Ursache selbständig
  aus Code + Logs erschließen. Für andere Module in diesem Repo (Touchbar,
  Trackpad/Maus, SMC, Gmux, Laden, ...) kann es hingegen sehr wohl relevante
  öffentliche Quellen/Issues in der t2linux-Community geben – dort ist
  Internet-Recherche sinnvoll und erlaubt.
- Der Nutzer kennt sich mit Hardware-nahen Internals nicht aus und testet nur
  das Verhalten zurück.
- Beim Schreiben/Ändern von Kernel-Modul-Code grundsätzlich großzügig
  Debug-Log-Marker einbauen (`pr_info`/`pr_warn` mit Port-/Device-Nummer,
  rohem Status/State, Retry-Zähler etc.), überall wo es für das Verständnis
  des jeweiligen Modul-Verhaltens sinnvoll ist. Jedoch muss auch drauf
  geachtet werden, dass es nicht zu einem Log-Overflow kommt und dadurch
  Logzeilen verloren gehen. Es muss eine sinnvolle Balance gefunden werden.
  Unnützes Debug-Logging soll immer wieder deaktiviert werden.
- Wichtige Erkenntnisse, Meilensteine, Sackgassen und andere Learnings müssen
  immer auf Englisch dokumentiert werden – in `LEARNINGS.md` für
  t2bce/Suspend-Resume-Themen, sonst im jeweils passenden/naheliegenden
  Dokument für das betroffene Modul.
- Commits, die sich als wirkungslos herausgestellt haben, sollen reverted
  werden. Wir wollen nur nachweislich nützliche Commits behalten und eine
  möglichst saubere Codebasis haben. Falls es jedoch einen Grund gibt, sie zu
  behalten, darf davon abgewichen werden.
- Kommentare im Code: idealerweise gar keine, höchstens Einzeiler. Nur
  kommentieren, was sich nicht aus dem Code selbst ergibt – und dann in einer
  Zeile. Keine mehrzeiligen Kommentarblöcke, keine "wall of text". Gilt nicht
  für Debug-Logging (siehe oben) und nicht für die Learnings-Dokumente.
- Commit-Messages: nur die Betreffzeile, kein Body. Wenn ein Body zwingend
  nötig erscheint, gehört der Inhalt stattdessen in das passende
  Learnings-Dokument. Keine erkennbar AI-generierten Commit-Messages – der
  Project Owner mag die nicht.
- Niemals `Co-Authored-By` (oder ähnliche Trailer) in Commit-Messages setzen.
- Commit-Messages sind nach dem Merge faktisch unveränderlich: Ändern würde
  den Hash ändern und einen Force-Push auf das geteilte Repo
  `kaiT2en/KaiT2en-Fedora` erfordern – das ist keine Option. Die Message muss
  also beim ersten Mal stimmen. (Passiert bei `68585ed`, dessen zu langer,
  AI-generierter Body nicht mehr korrigierbar war.)
