# COPR-Testkernel-Pipeline für KaiT2en

## Context

Ein Teil der Patches (Thunderbolt-`acpi.c`/`pci.c`, Bytes tief integrierte Sachen)
lässt sich nicht als DKMS-Modul bauen – es muss ein ganzer Kernel kompiliert
werden. Heute heißt das: jeder Tester kocht seinen Mac zwei Stunden lang, und
Patches werden per Discord als `.patch`-Dateien herumgereicht. Andres neues
`scripts/fedora-kernel-build-script/build.sh` (nur auf `upstream/main`, Commit
`ffa54de`) drückt das auf ~45 min, löst aber das eigentliche Problem nicht:
Tester sollen gar nicht selbst bauen müssen.

Ziel: **Patch-Datei in einen PR → COPR baut → PR-Kommentar mit Build-URL →
nach Fertigstellung PR-Kommentar mit fertigen Installationsbefehlen.** Tester
installieren nur noch ein RPM. Das betrifft ausdrücklich nur Tester/Devs –
für normale Nutzer bleibt es bei „Vanilla Fedora, DKMS-Module“
(`website/docs/post-install/updating.md` bleibt unverändert).

## Empfohlener Weg in einem Satz

Fedoras offizielles `kernel.spec` per SRPM-Rebuild um unsere Patches
(`linux-kernel-test.patch`) und die T2-Config-Abspeckung (`kernel-local`)
erweitern, das erzeugte SRPM aus GitHub Actions an COPR schicken und den
PR-Status per `gh` kommentieren.

---

## Warum dieser Weg (kurz)

Beide Einstiegspunkte in Fedoras Kernel-Spec sind verifiziert
(`gitlab.com/cki-project/kernel-ark`, Branch `fedora-7.1`):

- `Patch999999: linux-kernel-test.patch` + `ApplyOptionalPatch linux-kernel-test.patch`
  in `%prep` – der von Fedora vorgesehene Platz für Fremdpatches.
- `Source3001: kernel-local` – wird per `merge.py` in die Configs gemischt,
  der vorgesehene Platz für Config-Overrides.
- `%define with_debug/_without_debug`-Style-Bconds für `debug`, `debuginfo`,
  `perf`, `libperf`, `tools`, `doc`, `headers`, `cross_headers`, `selftests`,
  `kabichk`, `kernel_abi_stablelists` – alle abschaltbar, spart massiv Zeit
  und COPR-Speicher.

Damit fällt der Basis-Tree identisch aus wie bei `build.sh` (Vanilla-Tarball +
`patch-*-redhat.patch`), d.h. Patches, die lokal mit `build.sh` gehen, gehen
auch in COPR. Anders als `build.sh`s `make binrpm-pkg` bekommen wir aber echte
Fedora-NVRs, `kernel-devel`, `kernel-core`, `%{?dist}`-Tag und funktionierende
`kernel-install`-Scriptlets (BLS-Eintrag + initramfs automatisch).

## COPR-Setup (einmalig, außerhalb des Repos)

1. FAS-Gruppe `kait2en` im Fedora Account System beantragen, dann Projekt
   **`@kait2en/kernel-testing`** anlegen. Falls der Gruppenantrag hakt: erst
   persönliches `<lead>/kait2en-kernel-testing`, aber bewusst entscheiden –
   COPR-Projekte lassen sich später nicht umziehen.
2. Projektkonfiguration:
   ```
   copr-cli create @kait2en/kernel-testing \
     --chroot fedora-44-x86_64 \
     --delete-after-days 30 \
     --timeout 18000 \
     --description "Patched test kernels for T2 Macs. Testers only, not for production."
   ```
3. Dediziertes Bot-FAS-Konto als Projekt-Admin hinzufügen, dessen
   `~/.config/copr` als GitHub-Secret `COPR_CONFIG` hinterlegen (nicht den
   persönlichen Token des Lead).

**Zur Frage „mehrere Kernel gleichzeitig?“ – ja.** Ein COPR-Projekt kann
beliebig viele Kernel parallel vorhalten, solange sich die NVR unterscheidet.
Genau dafür setzen wir `%define buildid`:

```
kernel-7.1.9-200.kait2en.pr41.g1a2b3c4.fc44.x86_64   ← Thunderbolt-Serie
kernel-7.1.9-200.kait2en.pr42.g9f8e7d6.fc44.x86_64   ← Bytes Patch
```

Beide liegen gleichzeitig im Repo, `kernel` ist ein `installonlypkg`, also
können auch beide gleichzeitig *installiert* sein. Tester wählen mit der
vollen NVR aus. Nebeneffekt und gewollt: ein neuerer Fedora-Kernel
(`7.1.10-200.fc44`) sticht alte Testkernel beim `dnf upgrade` wieder aus.

Retention bewusst über `--delete-after-days 30` statt `--max-builds N`: ein
Zähler-Limit würde sonst einen Kernel wegräumen, den gerade noch jemand testet.

## Ablauf der Patches (Discord-Problem)

Empfehlung, in dieser Reihenfolge:

1. **Byte & Co. bekommen Push-Rechte auf `kaiT2en/KaiT2en-Fedora`** und legen
   selbst einen Branch + PR mit ihrer `.patch` an. Nur so läuft der Build
   vollautomatisch – GitHub gibt Workflows aus *Fork*-PRs grundsätzlich weder
   Secrets noch Schreibrechte, der COPR-Token und das PR-Kommentieren
   funktionieren dort prinzipbedingt nicht.
2. Kommt trotzdem etwas über Discord: ein Maintainer legt den Branch an, wirft
   die Datei nach `scripts/fedora-kernel-build-script/patches/` und öffnet den
   PR. Ein Handgriff, danach automatisch.
3. Für Einzelfälle ohne PR: `workflow_dispatch` mit Branch-Auswahl.

Fork-PR-Automatik (zweistufig über `workflow_run`) bewusst *nicht* in V1 –
deutlich mehr YAML für einen Fall, den es aktuell nicht gibt.

---

## Umsetzung

### Vorbereitung

- `main` von `upstream` aktualisieren (enthält `build.sh` erst seit `ffa54de`),
  Themen-Branch `kernel-copr-ci` davon abzweigen.
- Learnings-Datei `COPR-KERNEL-CI-LEARNINGS.md` anlegen (untracked, englisch).

### 1. `scripts/copr/make-kernel-srpm.sh` (Kernstück, neu)

Baut aus Fedoras Kernel-SRPM unser Test-SRPM. Muss lokal *und* in CI laufen.

- Optionen: `--kernel-release <NVR.arch>` (default: neuestes aus den
  Fedora-Repos via `dnf download --source kernel`), `--buildid <suffix>`,
  `--patch-dir` (default `scripts/fedora-kernel-build-script/patches`),
  `--out <dir>`, `--full-config` (Abspeckung aus).
- Schritte:
  1. Fedora-Kernel-SRPM holen. Discovery **nicht** per HTML-Scraping wie
     `build.sh`, sondern wie `scripts/fedora/install-gpu-runtime-pm.sh:147`
     über `rpm -q --qf '%{SOURCERPM}'` bzw. `dnf download --source`.
  2. `rpm -i --define "_topdir $WORK"` → `SOURCES/` + `SPECS/kernel.spec`.
  3. Alle `$PATCH_DIR/*.patch` **in derselben Reihenfolge wie `build.sh`**
     (sortierter Glob, kein `series`) zu `SOURCES/linux-kernel-test.patch`
     konkatenieren. Gleiche Reihenfolge = lokal und CI erzeugen denselben Tree.
  4. Config-Fragment aus der Symbolliste (siehe 2.) erzeugen und an
     `SOURCES/kernel-local` anhängen.
  5. `SPECS/kernel.spec` bearbeiten: ganz oben `%global _without_debug 1` &
     Co. einfügen (`debuginfo`, `perf`, `libperf`, `tools`, `doc`, `headers`,
     `cross_headers`, `selftests`, `kabichk`, `kernel_abi_stablelists`),
     dazu `%define buildid .kait2en.<suffix>`. `kernel-devel` bleibt an.
  6. `rpmbuild -bs` → SRPM, Pfad + NVR + KVER auf stdout (bzw.
     `$GITHUB_OUTPUT`).
- `set -euo pipefail`, keine Interaktivität, kein `sudo`, kein State im
  Repo-Checkout (`mktemp -d`) – die vier Punkte, an denen `build.sh` heute
  CI-untauglich ist.

### 2. Config-Abspeckung als gemeinsame Quelle

`scripts/fedora-kernel-build-script/configs/t2-disable.list` (neu): eine
Symbol-Zeile pro abgeschaltetem Treiber – exakt die ~70 Symbole, die heute in
`build.sh`s `--t2-config`-Block inline stehen.

- `make-kernel-srpm.sh` rendert daraus `# CONFIG_X is not set`-Zeilen für
  `kernel-local`.
- `build.sh` liest dieselbe Datei und ruft weiter `scripts/config --disable`
  auf – ~5 Zeilen Änderung an Andres Skript, dafür eine einzige Wahrheit.
  Diese Änderung im PR explizit erwähnen, sie fasst sein frisches Skript an.

### 3. `.github/workflows/kernel-copr.yml` (neu)

Bewusst ohne Third-Party-Actions (`gh` ist auf den Runnern vorinstalliert,
`podman` ebenfalls, `copr-cli` kommt per `pip`) – passt zum Stil der
bestehenden Workflows.

- Trigger: `pull_request` mit `paths:` auf
  `scripts/fedora-kernel-build-script/patches/**`,
  `scripts/fedora-kernel-build-script/configs/**`, `scripts/copr/**`,
  `.github/workflows/kernel-copr.yml`; dazu `workflow_dispatch`.
- Gates, analog `pages.yml` (und konform zu `DOCS-LEARNINGS.md:323`, keine
  toten Fork-Workflows):
  `github.repository == 'kaiT2en/KaiT2en-Fedora'` **und**
  `github.event.pull_request.head.repo.full_name == github.repository`.
- `environment: copr` – trägt das `COPR_CONFIG`-Secret. Vorerst *ohne*
  Required Reviewer (= automatisch für interne PRs); wer später doch ein
  manuelles Gate will, trägt dort einfach Reviewer ein, ohne YAML anzufassen.
- `permissions: { contents: read, pull-requests: write }`,
  `timeout-minutes: 240`,
  `concurrency: kernel-copr-${{ github.event.pull_request.number }}` mit
  `cancel-in-progress: true`.
- Schritte:
  1. Secret-Guard nach Hausmuster (leer → `::notice::` + skip statt Fehler).
  2. Checkout PR-Head.
  3. SRPM bauen:
     `podman run --rm -v "$PWD:/repo" -v "$PWD/out:/out" fedora:44 bash /repo/scripts/copr/make-kernel-srpm.sh --buildid "pr${PR}.g${SHA::7}" --out /out`
     (ubuntu-Host + Container statt `container:`-Job, spart die
     Node-in-Fedora-Image-Fallstricke bei `actions/checkout`).
  4. `~/.config/copr` mit `umask 077` schreiben,
     `copr-cli build --nowait @kait2en/kernel-testing out/*.src.rpm`
     → Build-ID / URL.
  5. Startkommentar per `gh pr comment` mit Marker `<!-- kait2en-copr -->`;
     bei Folge-Pushes denselben Kommentar via
     `gh api --method PATCH /repos/{owner}/{repo}/issues/comments/{id}`
     aktualisieren statt neu zu posten.
  6. `copr-cli watch-build <id>`, danach Kommentar auf Erfolg (Install-Block)
     bzw. Fehler (Link auf `builder-live.log`) umschreiben.
  7. `if: cancelled()` → `copr-cli cancel <id>`, damit kein verwaister
     COPR-Build weiterläuft.

### 4. `scripts/copr/pr-comment.md.in` (neu)

Template mit `@COPR_PROJECT@ @NVR@ @KVER@ @BUILD_URL@`, gerendert per `sed` –
gleiche Konvention wie `packaging/installer/macos-release-bootstrap.sh.in`.
Inhalt bewusst knapp, rein faktisch, Copy-Paste-Blöcke – kein Fließtext
(README: PRs mit erkennbar KI-generiertem Text werden abgelehnt).

Der Install-Block muss die im Repo dokumentierten Fallstricke abdecken:

```bash
sudo dnf copr enable @COPR_PROJECT@
sudo dnf install kernel-@NVR@ kernel-devel-@NVR@
sudo bash scripts/fedora/install-kernel-args.sh      # BLS-Args (--update-kernel=ALL)
sudo dkms autoinstall -k @KVER@                      # T2-Module für den neuen Kernel
sudo dracut --force /boot/initramfs-@KVER@.img @KVER@ # force_drivers-Drop-in greift erst jetzt
sudo grubby --info=ALL                               # Index des neuen Eintrags ablesen
sudo grubby --set-default-index=<N>                  # NICHT --set-default <pfad> (Btrfs!)
sudo reboot
```

Zurück:
```bash
sudo dnf remove kernel-@NVR@ kernel-devel-@NVR@
sudo dnf copr disable @COPR_PROJECT@
```

Plus drei Warnhinweise:
- Bootet er nicht: im GRUB-Advanced-Menü den Fedora-Kernel wählen, sonst
  `kait2en-rescue` vom Live-USB.
- MacBookPro15,1: `install-gpu-runtime-pm.sh` funktioniert mit diesem Kernel
  **nicht** (das Skript braucht das SRPM auf kojipkgs, das es für einen
  COPR-Kernel nicht gibt) – die AMDGPU-Runtime-PM-Patches fehlen also.
- Kernel und Module sind unsigniert → Secure Boot muss aus sein (auf T2-Macs
  ohnehin der Fall).

### 5. Kleinkram, sonst still kaputt

- Neue Skripte in die hartcodierte `shell_files`-Liste in
  `scripts/tests/static-check.sh` eintragen – sonst laufen weder `bash -n`
  noch `shellcheck`. Bei der Gelegenheit auch `build.sh` selbst eintragen, das
  fehlt dort aktuell.
- `scripts/fedora-kernel-build-script/README.md` um einen Abschnitt „COPR test
  builds“ ergänzen. **Nicht** ins `website/`-Doku-Set – das sagt heute
  explizit „KAIT2EN installiert keinen eigenen Kernel“, und für Endnutzer
  bleibt das auch so.
- `configs/7.1.config` ist byte-identisch zu `configs/default.config`
  (gleicher Blob) – Dublette, kann im selben PR weg.

---

## Betroffene Dateien

| Datei | Art |
|---|---|
| `scripts/copr/make-kernel-srpm.sh` | neu, Kernstück |
| `scripts/copr/pr-comment.md.in` | neu, Kommentar-Template |
| `.github/workflows/kernel-copr.yml` | neu |
| `scripts/fedora-kernel-build-script/configs/t2-disable.list` | neu, gemeinsame Symbolliste |
| `scripts/fedora-kernel-build-script/build.sh` | Änderung: Liste statt inline |
| `scripts/fedora-kernel-build-script/README.md` | Abschnitt COPR |
| `scripts/tests/static-check.sh` | Allowlist erweitern |

## Verifikation

**M0 – manuell, bevor irgendein YAML geschrieben wird.** Das ist der Teil mit
dem echten Risiko; alles danach ist Verdrahtung.

1. `bash scripts/copr/make-kernel-srpm.sh --buildid test --out /tmp/…` ohne
   Patches → SRPM entsteht, `rpm -qp --qf '%{NVR}'` enthält `.kait2en.test`
   und `.fc44`.
2. Gegenprobe auf die drei Spec-Annahmen im tatsächlich heruntergeladenen
   SRPM (nicht nur im kernel-ark-Template):
   `grep -n 'linux-kernel-test\|kernel-local\|_without_debug' SPECS/kernel.spec`.
3. Mit Andres `0001-x86-cache-…patch`: `rpmbuild -bp` auf das erzeugte SRPM →
   `%prep` läuft durch, Patch appliziert. (Fedoras `ApplyPatch` nutzt `patch`;
   reine unified diffs sind ok, Renames/Binaries wären es nicht.)
4. `copr-cli build @kait2en/kernel-testing …src.rpm` von Hand → Laufzeit und
   RPM-Größe messen, damit Timeout und Retention belastbar gesetzt sind.
5. Ergebnis auf diesem Mac installieren, Kommentar-Block Zeile für Zeile
   abarbeiten, rebooten: bootet, `uname -r` zeigt den Testkernel, `dkms
   status` zeigt alle Module gebaut, Touch Bar/Trackpad/Audio funktionieren.

**M1 – Automatik.** Test-PR, der nur eine Patch-Datei anfasst: Build startet,
Kommentar erscheint mit Build-URL, wird nach Abschluss auf den Install-Block
umgeschrieben; zweiter Push aktualisiert denselben Kommentar statt einen
neuen zu posten; ein PR ohne Änderung an `patches/` löst nichts aus.

## Bekannte Risiken

- **Fedora ändert das Spec.** `linux-kernel-test.patch`/`kernel-local` sind
  stabile, dokumentierte Mechanismen, aber nicht garantiert. Deshalb Schritt 2
  der Verifikation als harter Check *im Skript*: fehlt einer der Anker, sauber
  abbrechen statt still einen Kernel ohne unsere Patches zu bauen. Genau dieser
  Fehler wäre sonst tagelang unsichtbar.
- **Abgespeckte Config verfälscht Tests.** Wenn ein Testkernel Ärger macht, ist
  als Erstes `--full-config` gegenzuprüfen, bevor der Patch verdächtigt wird.
- **Laufzeit im Actions-Job.** Der Job wartet bis zu ~2 h auf COPR. Für
  öffentliche Repos kostenlos und unter dem 6-h-Limit; falls es stört, lässt
  sich der Watch-Teil später in einen zweiten Job per `workflow_run` ziehen.
- Commit-Messages: nur Betreffzeile, kein Body, kein `Co-Authored-By`.
