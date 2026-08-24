# Live-USB Rescue: Auto-Chroot, Boot-Backup an Kernel-Updates, Restore aus dem Live-System

## Context

Der Project Owner möchte drei zusammengehörige Dinge: einen Auto-Chroot in den
Rescue-Optionen des Live-USB-Sticks, ein Backup-Skript das sich in
Kernel-Updates einhängt, und eine Möglichkeit, aus dem Live-System heraus
wiederherzustellen.

Die drei Ideen greifen ineinander, wenn man sie auf die Fehlerklasse zuschneidet,
die einen T2-Mac tatsächlich unbootbar macht: ein Kernel-Update, nach dem der
DKMS-Build scheitert oder die initramfs die T2-Input-Module verliert. Dann gibt
es keine Tastatur mehr — weder am LUKS-Prompt noch in der Konsole — und der
einzige Weg zurück führt über den Live-Stick. Genau da fehlt heute alles:

- `packaging/installer/grub.cfg.in` hat sechs Einträge, davon vier
  Troubleshooting-Varianten — aber alle starten nur den Live-Desktop. Kein
  Rescue-Werkzeug.
- Im Live-System liegen `kait2en-live-wifi`, `-bluetooth`, `-diagnostics` unter
  `/run/kait2en/`. `kait2en-live-diagnostics` sammelt zwar `lsblk -f` und
  `findmnt` ein, tut aber mit der vorhandenen Installation nichts.
- Es gibt im ganzen Repo **kein** `mount`, `chroot`, `cryptsetup` oder
  `btrfs` — die Erkennung fremder Root-Dateisysteme ist komplett neuer Code.
- Es gibt **keinerlei** Backup-, Snapshot- oder Restore-Logik. Der einzige
  Kernel-Hook ist `/etc/kernel/install.d/39-kait2en-dkms-cleanup.install` aus
  `scripts/fedora/install-dkms-modules.sh:52`.

Ergebnis nach diesem Plan: vor jedem Kernel-Update wird der Boot-Zustand
versioniert weggesichert; wenn der Rechner danach nicht mehr startet, bootet man
den Live-Stick in eine Rescue-Konsole, das Werkzeug findet und mountet die
Installation selbst (inkl. LUKS und beliebigem btrfs-Subvolume-Namen), und man
spielt das letzte funktionierende `/boot` mit zwei Tastendrücken zurück.

### Entscheidungen (vom Nutzer bestätigt)

- **Backup-Umfang:** nur Boot-Zustand (`/boot`, Fedora-Teil der ESP, Manifest).
  Kein btrfs-Root-Snapshot, kein Voll-Backup.
- **Rescue-Zugang:** CLI-Kommando *und* eigener GRUB-Eintrag.
- **Rescue-UI:** nummeriertes Textmenü plus nicht-interaktive Flags. Kein
  whiptail/dialog (gibt es im Live-Image nicht und wäre eine neue Abhängigkeit).

### Bewusst nicht im Scope

Voll-System-Backup, btrfs-Root-Rollback, automatisches Zurückspielen ohne
Nachfrage, Unterstützung für LVM. Begründung siehe „Offene Punkte".

---

## Phase 1 — Boot-Backup auf dem installierten System

Branch: `feat/boot-backup` von aktuellem `main`.
Learnings-Datei: `BOOT-BACKUP-LEARNINGS.md` (untracked, nie committen).

### 1.1 `packaging/installer/runtime/kait2en-backup` (neu)

Ein Skript, zwei Auslieferungswege (Phase 2 nimmt dasselbe File mit auf den
Stick). Deshalb folgt es der etablierten `--root DIR`-Konvention aus
`install-wifi-firmware.sh`/`install-bt-firmware.sh` statt selbst zu chrooten —
so funktioniert es unverändert auf dem laufenden System *und* auf einem
gemounteten Fremd-Root.

Subkommandos:

| Kommando | Wirkung |
|---|---|
| `create [--reason TEXT]` | Neue Generation anlegen |
| `list` | Generationen mit Zeitstempel, Kernel-Liste, Größe, Grund |
| `restore <id>` | Generation zurückspielen (typisierte Bestätigung nötig) |
| `verify [KVER]` | Prüft, ob die initramfs die T2-Input-Module enthält |
| `prune` | Retention anwenden |

Ablageort: `${root}/var/lib/kait2en/boot-backups/<UTC-Zeitstempel>/`.
Liegt auf btrfs (viel Platz), nicht auf `/boot` (bei uns ext4, ~1 GB, 59 % voll —
dort passt keine einzige Generation hin).

Inhalt einer Generation:

- `boot/` — der komplette Inhalt des `/boot`-Baums: `vmlinuz-*`,
  `initramfs-*.img`, `System.map-*`, `config-*`, `symvers-*`,
  `loader/entries/*.conf`, `grub2/`.
- `esp/EFI/fedora/` — nur der Fedora-Zweig der ESP. Apples Zweig
  (`EFI/APPLE`, `EFI/Boot` von macOS) wird **nie** angefasst; das ist eine harte
  Regel und gehört als Kommentar ins Skript.
- `manifest.txt` — `grubby --info=ALL`, `grubby --default-kernel`,
  `/etc/kernel/cmdline`, `/etc/default/grub`, `rpm -q kernel-core`,
  `dkms status`, `uname -r`, Zeitstempel, Grund.
- `lsinitrd-<kver>.txt` je Kernel — damit man später sieht, ob die T2-Module
  drin waren.

Kopiert wird mit `cp -a --reflink=auto`. Auf btrfs kostet eine Generation
dadurch fast nichts, weil unveränderte initramfs-Dateien als Reflink geteilt
werden; auf ext4/xfs fällt es transparent auf eine echte Kopie zurück.

Retention: 3 Generationen, überschreibbar über
`/etc/kait2en/boot-backup.conf` (`generations=3`). Vor `create` eine
Platzprüfung (`df --output=avail`); zu wenig Platz → Warnung und `exit 0`, nie
Abbruch.

Stil: Kopfkommentar-Absatz, `set -Eeuo pipefail`, `shopt -s nullglob`,
`log()`/`die()` mit `[kait2en]`-Präfix, alle Pfade als
`${KAIT2EN_*:-default}`-Variablen (damit die Tests sie faken können),
`trap cleanup EXIT`, handgeschriebenes `while (($# > 0)); do case "$1" in`
mit `usage()`. Vorlage: `packaging/installer/runtime/kait2en-live-diagnostics`.

`restore` verlangt — wie `select_disk()` in
`scripts/macos/prepare-fedora-installer.sh:325` — eine getippte Bestätigung
(`RESTORE`) in einer Schleife, und legt vorher automatisch eine
`create --reason "before restore"`-Generation an.

### 1.2 `/etc/kernel/install.d/09-kait2en-boot-backup.install` (neu)

Wird von `scripts/fedora/install-boot-backup.sh` geschrieben, genau nach dem
Muster von `install_dkms_kernel_hook()` in
`scripts/fedora/install-dkms-modules.sh:52` (mktemp → heredoc → `install -m 0755`).

```
[[ "$command" == add && -n "$kernelver" ]] || exit 0
/usr/local/bin/kait2en-backup create --reason "kernel-install add $kernelver" || :
exit 0
```

Index `09`, damit er vor allem läuft, was `/boot` schreibt (`20-grub`,
`50-dracut`, `51-dracut-rescue`, `90-loaderentry`). Er sichert also den letzten
funktionierenden Zustand, bevor er überschrieben wird.

**Harte Regel: der Hook darf die Kernel-Transaktion nie zum Scheitern bringen.**
Deshalb `|| :` und `exit 0` am Ende — identisch zum vorhandenen 39er-Hook.

### 1.3 `/etc/kernel/install.d/99-kait2en-boot-verify.install` (neu)

Läuft ganz am Ende von `add` und ruft `kait2en-backup verify "$kernelver"`.
Prüft mit `lsinitrd` auf `t2bce_dma`, `t2bce_core`, `t2bce_vhci`, `t2hid` — die
Verifikationslogik gibt es bereits in `kait2en-prepare:259-263`, sie wird von
dort übernommen. Bei Fehlen: laute Warnung mit dem konkreten
Wiederherstellungskommando. Auch hier `exit 0`, nie Abbruch.

Das ist das Stück, das den Nutzer wirklich rettet: es sagt ihm *vor* dem Reboot,
dass die Tastatur weg sein wird.

### 1.4 `scripts/fedora/install-boot-backup.sh` (neu)

`source lib.sh`, dann `require_root`/`require_repo_root`/`require_fedora`/
`require_command`. Installiert:

- `packaging/installer/runtime/kait2en-backup` → `/usr/local/bin/kait2en-backup`
- die beiden Hooks nach `/etc/kernel/install.d/`
- `/etc/kait2en/boot-backup.conf` (nur anlegen, wenn nicht vorhanden)

und legt zum Schluss eine erste Generation an, damit sofort ein bekannt guter
Stand existiert.

### 1.5 Anschluss

- `scripts/fedora/install.sh`: `install-boot-backup.sh` in `STEPS` aufnehmen,
  **nach** `rebuild-initramfs.sh` (dann ist die initramfs schon korrekt und die
  erste Generation ist auch wirklich gut).
- `scripts/tests/static-check.sh`: beide neuen Skripte in `shell_files`
  aufnehmen; Invarianten ergänzen:
  - `grep -Fq 'exit 0'` bzw. explizite Zusicherung, dass der Hook nicht fehlschlägt
  - `! grep -InE 'EFI/(APPLE|Boot)' packaging/installer/runtime/kait2en-backup`
  - `grep -Fq 'reflink=auto' packaging/installer/runtime/kait2en-backup`
- `scripts/tests/boot-backup.sh` (neu): Fake-Root unter `mktemp -d`, gefakte
  `grubby`/`dracut`/`lsinitrd`/`dkms` in einem `$case_dir/bin` auf `PATH`,
  alle Pfade über `KAIT2EN_*`. Deckt ab: create/list/prune-Retention,
  Restore-Round-Trip, `verify` schlägt bei fehlendem Modul an, ESP-Apple-Zweig
  bleibt unberührt, Platzmangel führt zu Warnung statt Fehler.
  Vorlage: `scripts/tests/live-wifi.sh`.

---

## Phase 2 — Auto-Chroot und Restore im Live-System

Branch: `feat/live-rescue` von `main`, nach dem Merge von Phase 1.
Learnings-Datei: `INSTALLER-RESCUE-LEARNINGS.md` (untracked).

### 2.1 `packaging/installer/runtime/kait2en-live-rescue` (neu)

Kein systemd-Unit für den normalen Boot — ein fremdes Root zu mounten darf nie
automatisch passieren. Aufruf: `sudo /run/kait2en/kait2en-live-rescue`.

**Erkennung** (kompletter Neubau, aber nach vorhandenem Idiom: Glob →
`[[ ... ]] || continue` → `printf`/`return 0`, wie `bcm4377_slot()` in
`kait2en-live-bluetooth` und `pick_target()` in `kait2en-live-diagnostics`):

1. Live-Medium ermitteln (`findmnt -no SOURCE /run/initramfs/live`) und dessen
   ganzes Elterngerät von der Kandidatenliste ausschließen.
2. `lsblk -rno PATH,FSTYPE,LABEL,UUID,SIZE,TYPE` durchgehen. **`apfs` und
   `hfsplus` werden hart übersprungen** — die macOS-Partition darf nie
   Kandidat werden. Kandidaten sind `btrfs`, `ext4`, `xfs`, `crypto_LUKS`.
3. `crypto_LUKS` → anbieten, mit `cryptsetup open` zu entsperren; danach das
   Mapper-Device erneut prüfen.
4. Jeden Kandidaten read-only probemounten. Bei btrfs jedes Top-Level-Subvolume
   aus `btrfs subvolume list -o` durchprobieren — der Subvolume-Name ist **nicht**
   fix `root`; `INSTALLER-BLS-DEFAULT-LEARNINGS.md` dokumentiert einen Nutzer
   mit `@`. Fedora-Root ist, was `etc/os-release` mit `ID=fedora` und
   `etc/fstab` hat. Wieder aushängen.
5. Nummeriertes Menü der Fundstellen, Format wie `select_disk()`
   (`  %2d) %s — %s (%s)`), mit `Fedora <VERSION_ID>`, Gerät, Subvolume, Größe.

**Mounten** nach `/mnt/kait2en-rescue`:

- Root rw mounten (btrfs mit dem gefundenen `subvol=`).
- Alles Weitere aus der fstab **des Ziels** ableiten:
  `findmnt --fstab -F "$target/etc/fstab" -no TARGET,SOURCE,FSTYPE,OPTIONS`,
  nach Pfadtiefe sortiert mounten, `swap` und Netz-Dateisysteme überspringen.
  Das erledigt beide `/boot`-Layouts (eigene Partition *oder* Verzeichnis im
  Root-Subvolume, siehe `INSTALLER-BLS-DEFAULT-LEARNINGS.md`) und jeden
  Subvolume-Namen, ohne dass wir raten.
- Bind-Mounts: `/dev`, `/dev/pts`, `/proc`, `/sys`, `/run`, und
  `/sys/firmware/efi/efivars` rw (sonst kann `grub2-install`/`efibootmgr` im
  chroot nichts tun).
- `/etc/resolv.conf` sichern und die des Live-Systems hineinkopieren, damit
  `dnf`/`git` im chroot Netz haben; beim Abbau zurücklegen.
- Abbau vollständig in einem `trap cleanup EXIT`: `umount -R` in umgekehrter
  Reihenfolge, `cryptsetup close`, `rmdir`.

**Menü / Aktionen** (jede auch als Flag):

| Menü | Flag | Wirkung |
|---|---|---|
| 1 | `--shell` | `chroot` in eine interaktive Bash |
| 2 | `--rebuild-initramfs` | `dracut --regenerate-all --force` im chroot, danach `lsinitrd`-Verifikation der vier Input-Module (Muster aus `kait2en-prepare:259-263`) |
| 3 | `--repair-boot` | `grubby --info=ALL` anzeigen, Default per `grubby --set-default-index` setzen (**nicht** per Pfad — `--set-default <pfad>` scheitert bei `/@/boot/...`-Layouts, siehe `INSTALLER-BLS-DEFAULT-LEARNINGS.md`); `boot_entry_index()` aus `kait2en-prepare:73` wiederverwenden; leeres `GRUB_CMDLINE_LINUX` über die Logik von `repair_empty_grub_cmdline()` aus `scripts/fedora/install-kernel-args.sh:42` reparieren |
| 4 | `--restore` | `kait2en-backup --root /mnt/kait2en-rescue list`, Auswahl, dann `restore` |
| 5 | `--reinstall` | `bash /usr/local/src/KaiT2en-Fedora/scripts/fedora/install.sh` im chroot; nur anbieten, wenn der Checkout existiert und Netz da ist |
| 6 | `--diagnostics` | `kait2en-live-diagnostics` laufen lassen und zusätzlich den Zustand des Ziels einsammeln |

Vor jeder schreibenden Aktion die getippte Bestätigung; `--shell` und
`--diagnostics` brauchen keine.

`kait2en-backup` aus Phase 1 wird mit auf den Stick genommen und mit
`--root /mnt/kait2en-rescue` aufgerufen — nicht die Kopie aus dem Ziel, damit
Restore auch dann geht, wenn das Ziel-`/usr/local/bin` beschädigt ist.

### 2.2 Auslieferung auf den Stick

- `packaging/installer/build-in-container.sh`: zwei `install -m 0755`-Blöcke
  neben dem für `kait2en-live-diagnostics` (~Zeile 268), Ziel
  `usr/lib/kait2en/kait2en-live-rescue` und `usr/lib/kait2en/kait2en-backup`;
  dazu die passenden Selbstprüfungen im `cpio -it | grep -Fx`-Block (~Zeile 316).
- `packaging/installer/initramfs/90-kait2en-updates.sh`: beide Namen in die
  `for helper in …`-Liste (Zeile 24-25) aufnehmen.
- Neues Unit `kait2en-live-rescue.service` in denselben Mechanismus (Zeile 35),
  aber mit `ConditionKernelCommandLine=kait2en.rescue` — nach dem Vorbild von
  `ConditionPathIsDirectory=` in `kait2en-live-wifi.service`. Ohne den
  Kernel-Parameter startet es nie, der normale Live-Boot bleibt unverändert.
  Unit-Eckdaten: `Conflicts=getty@tty1.service`, `Type=idle`,
  `StandardInput=tty`, `TTYPath=/dev/tty1`, `ExecStart=/run/kait2en/kait2en-live-rescue`.

### 2.3 GRUB-Eintrag

In `packaging/installer/grub.cfg.in` ein siebter Eintrag:

```
menuentry "Rescue: repair an installed KaiT2en system" ... {
	linux ($fedora)@ISO_KERNEL_PATH@ ${kait2en_common} module_blacklist=${kait2en_blacklist} plymouth.enable=0 systemd.unit=multi-user.target kait2en.rescue
	initrd ${kait2en_initrd}
}
```

Zwei Punkte, die zwingend so sein müssen:

- Der Eintrag muss `${kait2en_common}` **und** `${kait2en_initrd}` benutzen.
  `scripts/tests/static-check.sh:63-68` zählt beide und verlangt
  `entries == overlays` — ein Eintrag ohne die Input-initramfs hätte keine
  Tastatur, also ausgerechnet im Rescue-Fall nichts.
- Kein GUI: `systemd.unit=multi-user.target`. Genau dann braucht man das
  Rescue, wenn der Desktop nicht kommt. Und laut
  `INSTALLER-GRUB-MENU-LEARNINGS.md` kann man auf einem T2-Mac Boot-Optionen
  nicht editieren (Apples EFI liefert kein `EFI_SIMPLE_TEXT_INPUT_EX_PROTOCOL`,
  also kein Ctrl-X/F10) — jede Variante braucht ihren eigenen Eintrag.

### 2.4 Tests

- `scripts/tests/live-rescue.sh` (neu): Fake-`lsblk`/`findmnt`/`mount`/`umount`/
  `cryptsetup`/`chroot`/`btrfs` in `$case_dir/bin`, Steuerung über `KAIT2EN_*`.
  Fälle: APFS-Partition wird nie Kandidat; Live-Medium wird ausgeschlossen;
  btrfs-Subvolume `@` wird gefunden; `/boot` als Verzeichnis im Root-Subvolume
  funktioniert; LUKS-Pfad; Cleanup hängt alles wieder aus, auch nach Fehler.
- `static-check.sh`: neue Skripte in `shell_files`; Invarianten:
  `grep -Fq 'usr/lib/kait2en/kait2en-live-rescue' build-in-container.sh`,
  `! grep -InE 'apfs' …` bzw. positiv `grep -Fq 'apfs'` als Skip-Prüfung im
  Rescue-Skript, `grep -Fq 'set-default-index' kait2en-live-rescue`,
  `! grep -Fq -- '--set-default ' kait2en-live-rescue`.

---

## Dokumentation (beide Phasen)

- `packaging/installer/README.md`, Schritt 3: `kait2en-live-rescue` neben den
  drei bestehenden `sudo /run/kait2en/...`-Kommandos nennen, plus den neuen
  GRUB-Eintrag.
- `website/docs/installation.md`: Abschnitt „If the installed system no longer
  starts" mit dem konkreten Ablauf (Stick booten → Rescue-Eintrag → Ziel wählen
  → Backup zurückspielen).
- `website/docs/post-install/backup-and-rescue.md` (neu) plus Eintrag in
  `website/site.yml` (Liste bei Zeile 89-92 und die Redirect-Map bei Zeile 105-108).
- `website/docs/reference/installed-files.md`: `/usr/local/bin/kait2en-backup`,
  `/var/lib/kait2en/boot-backups/`, `/etc/kait2en/boot-backup.conf` und die
  beiden neuen `/etc/kernel/install.d/`-Hooks ergänzen (der 39er-Hook steht
  dort schon bei Zeile 47-49).

---

## Verifikation

**Phase 1, direkt lokal auf diesem Mac:**

1. `bash scripts/tests/static-check.sh` — muss mit `shellcheck` durchlaufen
   (liegt unter `/usr/bin/shellcheck`), sonst wird die Stufe still übersprungen.
2. `bash scripts/tests/boot-backup.sh`.
3. `sudo bash scripts/fedora/install-boot-backup.sh`, dann
   `sudo kait2en-backup list` — eine Generation muss existieren.
4. Echtes Kernel-Update abwarten oder erzwingen:
   `sudo kernel-install add $(uname -r) /boot/vmlinuz-$(uname -r)`. Danach muss
   eine zweite Generation dastehen und `journalctl` den Verify-Hook zeigen.
5. Platzkontrolle: `du -sh /var/lib/kait2en/boot-backups` und
   `sudo compsize /var/lib/kait2en/boot-backups` — die Reflinks müssen greifen,
   sonst kostet jede Generation den vollen `/boot`-Umfang.
6. Restore-Probe: `sudo kait2en-backup restore <id>`, danach
   `sudo kait2en-backup verify` und **Reboot** — das ist der einzige echte
   Beweis, dass der Boot-Zustand konsistent ist.

**Phase 2 — braucht einen gebauten Stick, nicht nur lokale Tests:**

1. `bash scripts/tests/live-rescue.sh` und `static-check.sh`.
2. `./scripts/fedora/build-installer.sh --target fedora-44`, Stick mit
   `scripts/macos/prepare-fedora-installer.sh` schreiben.
3. Von diesem Stick booten, den Rescue-Eintrag wählen: es muss eine Textkonsole
   mit dem Menü kommen, **und die interne Tastatur muss funktionieren** (das ist
   die eigentliche Regressionsgefahr — Beweis, dass die Input-initramfs-Overlays
   im neuen Eintrag wirklich geladen wurden).
4. Im Menü: Ziel erkennen lassen und prüfen, dass die APFS-Partition nicht
   auftaucht; Ziel mounten; `--shell` und darin `ls /boot`, `cat /etc/fstab`;
   Menü verlassen und mit `findmnt` prüfen, dass nichts gemountet blieb.
5. Restore aus dem Live-System durchspielen und ins Ziel booten.

Für Schritt 3-5 brauche ich deine Rückmeldung — das ist der Teil, den ich nicht
selbst testen kann.

---

## Offene Punkte

- **Reihenfolge:** Phase 1 muss zuerst landen; Phase 2 konsumiert
  `kait2en-backup`. Zwei getrennte Branches, weil es zwei Probleme sind (so
  will es `CLAUDE.md`).
- **Root-Snapshots bewusst ausgelassen.** `/boot` ist hier ext4 und damit von
  btrfs-Snapshots gar nicht erfasst — ein Root-Snapshot allein hätte die
  Fehlerklasse also nicht abgedeckt. Umgekehrt deckt das Boot-Backup den Fall
  „kaputte Datei in `/usr`" nicht ab. Wenn das später gewünscht ist, ist der
  saubere Weg `snapper` plus dessen dnf-Plugin (auf dieser Maschine bereits
  installiert) statt Eigenbau — als separater, optionaler Installationsschritt.
- **LVM wird nicht unterstützt.** Kommt im ganzen Repo nicht vor und Anaconda
  legt es bei unserer dokumentierten Anleitung nicht an. Das Rescue-Tool wird
  einen LVM-PV erkennen und mit einer klaren Meldung ablehnen, statt still
  danebenzugreifen.
