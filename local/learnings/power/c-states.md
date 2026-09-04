# Package-C-State-Diagnose (MacBookPro16,2 / KaiT2en Fedora 44)

## Ziel und Leitplanken

- Root Cause dafür bestimmen, dass tiefe Package-C-States (insbesondere PC8–PC10) nur sehr selten erreicht werden.
- Zunächst ausschließlich beobachten und messen: keine Tunables, Dienste, Module, Geräte oder Kernelparameter verändern.
- Diese Datei bleibt lokal/untracked und dient als fortlaufendes Laborjournal.
- Diagnose-Branch: `tests/c-states`.

## Bekannter Ausgangspunkt (Angaben des Maintainers)

- Hardware: T2 MacBookPro16,2.
- Betriebssystem: KaiT2en Fedora 44.
- Package-C-State-Anzeige in PowerTOP zuletzt ungefähr: C2 23,6 %, C6 0,5 %, C7 0,7 %.
- Tiefe States sind grundsätzlich erreichbar; nach einer frischen Installation wurden zeitweise etwa 50 % Package C8 beobachtet.
- `sudo powertop --auto-tune` setzt alle angezeigten Tunables auf `Good`, senkt aber weder den tatsächlichen Verbrauch noch verbessert es die Package-C-State-Verteilung. Es wurde wegen vermuteter Instabilität nicht persistiert.
- GNOME Power Mode `Power Saver` brachte keine erkennbare Verbesserung.

## Methodik

1. Unveränderten Ist-Zustand erfassen (Kernel, Bootparameter, CPU-idle, PM-QoS, Power-Profil, PCI/USB, Wakeups, Interrupts).
2. Zeitfenster-Messungen durchführen und Residency-Zähler statt einzelner UI-Snapshots vergleichen.
3. Blocker-Hypothesen nach Evidenz priorisieren.
4. Erst nach ausdrücklicher Freigabe jeweils genau eine Variable verändern, erneut messen und danach möglichst zurücksetzen.

## Testprotokoll

| Zeitpunkt | Test / Beobachtung | Systemzustand | Ergebnis | Interpretation / nächster Schritt |
|---|---|---|---|---|
| 2026-08-05 | Diagnosejournal und Branch angelegt | Keine System-Tunables verändert | Branch `tests/c-states`; Datei untracked | Baseline-Erfassung folgt |
| 2026-08-05 22:32 | Statische CPU-idle-Baseline | Akku, ADP1 offline; laufende GNOME-Sitzung | `intel_idle`, Governor `menu`; alle 4 angebotenen States aktiv (`POLL`, `C1_ACPI`, `C2_ACPI`, `C3_ACPI`); `intel_idle.max_cstate=9` | Kein deaktivierter Core-idle-State; der Parameter ist kein direkter PC10-Schalter |
| 2026-08-05 22:33 | `turbostat --Summary`, 4 x 5 s | PowerTOP und normale Desktop-Apps liefen parallel | Busy 11,5–14,3 %; `Any%C0` 57–63 %; PC2 6,7–9,3 %; PC6 0,07–0,13 %; PC7–PC10 0 %; ca. 4.450–5.740 IRQ/s; Package 4,5–5,2 W | Aktuelle Messung ist kein Idle-Test. Selbst wenn alle Cores kurz schlafen, wird fast nie PC6+ erreicht |
| 2026-08-05 22:35 | Host-`pidstat`, 15 s | Interaktives PowerTOP weiterhin offen; Codex-Diagnose aktiv | Zeitweise u. a. PowerTOP ~26 %, Ptyxis ~15 %, GNOME Shell ~11–17 %, Firefox ~5–7 %, Codex ~5–8 % eines Cores; viele i915-Workqueues | Interaktives PowerTOP/Ptyxis erzeugt einen erheblichen Observer-Effekt; künftige Idle-Baseline ohne PowerTOP-UI erfassen |
| 2026-08-05 22:36 | Intel-PMC-Zähler | Seit Boot kumuliert | PC2 355.892.205, PC3 394.683, PC6 9.058.588, PC7 2.585.898, PC8 3.519, PC9/PC10 0; S0ix-Residency 0 | Hardware bestätigt extrem wenig PC8 und kein PC9/10 in diesem Boot; absolute Einheiten/Bootdauer noch normieren |
| 2026-08-05 22:36 | Intel-PMC Power-Gating/LTR | Desktop aktiv, daher Snapshot nicht als Idle-Latch interpretieren | XHCI und LPSS aktuell `On`; zahlreiche Fabric-Blöcke ebenfalls `On`; kein offensichtlicher gültiger enger LTR-Wert erkannt | Im echten Idle-Fenster erneut lesen; `slp_s0_debug_status` bezieht sich primär auf S0ix und ist im aktiven Desktop nicht direkt beweiskräftig |
| 2026-08-05 22:37 | PCI Runtime-PM | Unverändert | TB-xHCI `00:0d.0` suspendiert/D3; vier leere TB-Root-Ports suspendiert/D3; beide TB-NHIs `00:0d.2/.3` seit Boot durchgehend aktiv/D0 trotz `power/control=auto`; WLAN, ANS2-NVMe und alle T2-Funktionen ebenfalls aktiv/D0 | TB-NHIs sind ein konkreter Uncore-Blocker-Kandidat; T2/ANS2/WLAN müssen wegen funktionaler Nutzung getrennt bewertet werden |
| 2026-08-05 22:38 | PCIe ASPM/L1SS | ASPM policy `[default]` | WLAN-Pfad hat L1.2 aktiv. T2/ANS2-Pfad hat nur L1.1; Root Port `00:1c.4` meldet gar keine L1.2-Fähigkeit. Alle vier leeren TB-Root-Ports nutzen L1/D3 | PowerTOP-Autotune kann diesen strukturellen Unterschied nicht beheben. Da PC8 historisch erreichbar war, ist T2-L1.1 allein noch kein ausreichender Root Cause |
| 2026-08-05 22:39 | KaiT2en `t2thunderbolt` Source Review | Modul v0.3 geladen | Auf Ice Lake werden je NHI PM-Runtime-Device-Links zu **allen vier** TRP-Root-Ports angelegt (insgesamt 8 Links); keine D3-Sperre für ICL. Sysfs bestätigt, dass jeder Port beide NHIs als Supplier hat | Topologiezuordnung ist sehr breit und prüfenswert; noch nicht bewiesen, dass sie NHIs aktiv hält |
| 2026-08-05 22:39 | ACPI-GPE-Zähler | Seit Boot kumuliert | GPE `6E`: 144.110 von 144.594 GPEs; GPE `66`: 483 | GPE6E dominiert ACPI-Wakeups (~6/s seit Boot); AML-Methode und aktuelle Rate ermitteln |
| 2026-08-05 22:44 | Read-only Messskript, 15 s | PowerTOP bereits beendet, normale aktive GNOME-/Terminal-/Browser-Sitzung | Busy 15,67 %, `Any%C0` 62,23 %, PC2 7,42 %, PC3 0,01 %, PC6 0,11 %, PC7–PC10 0 %, Package 5,31 W; 88.931 IRQs/15 s (~5.929/s) | Auch ohne PowerTOP-UI weiterhin kein Idle. Größte ausgewählte IRQ-Deltas: i915 455/s, BCE-DMA 181/s, NVMe 173/s, WLAN 24,6/s, ACPI/GPE6E 11,1/s |
| 2026-08-05 22:45 | T2-BCE Source Review | Keine Änderungen | `bce_xhci_pm_tick()` läuft ab Probe alle 150 ms, liest MMIO, liest Boot-Time und schreibt zwei MMIO-Werte; Timer wird selbst erneut geplant | Garantierte 6,67 Wakeups/s plus MMIO-Zugriffe. Konkreter Optimierungskandidat, aber allein zu klein für die derzeitige Last und noch nicht als Package-C-State-Blocker bewiesen |
| 2026-08-05 22:46 | Fan-Daemon Source Review | `t2-fancontrol.service` aktiv | Daemon-Hauptschleife wacht alle 100 ms auf; eigentliche Temperatur-/Regelabfrage nur alle 2 s. Seit Boot nur ~33 s CPU-Zeit | Garantierte 10 Userspace-Wakeups/s, davon PowerTOP später 3,9/s als wirksam attribuiert. Verstärker, nicht wahrscheinlicher alleiniger Root Cause |
| 2026-08-05 22:47 | Intel-Display-Diagnose (i915 debugfs) | Internes eDP bei 2560x1600@60, 30 bpp/XR30 | DMC-Firmware 1.9 geladen; seit Boot nur 2 Übergänge DC3→DC5 und **0 DC5→DC6**. PSR-Diagnose liefert `ENODEV`; Panel-DPCD ist 1.1. FBC ist wegen nicht unterstütztem XR30-Pixelformat aus | Display bleibt energetisch auffällig flach. PSR ist bei diesem Panel offenbar nicht verfügbar; DC6=0 ist ein starker Befund, aber PC8 setzt nicht pauschal PSR voraus und der kausale Anteil muss per Display-off-Test bestimmt werden |
| 2026-08-05 22:48 | PM-QoS und TuneD | Profil `balanced-battery`, dynamisches Tuning global aus | Nur TuneD hält `/dev/cpu_dma_latency` offen. Weder `balanced` noch `balanced-battery` setzen `force_latency` oder `pm_qos_resume_latency_us`; CPU-idle-Sysfs zeigt keine per-device Resume-Latency-Anforderung. `tuned-adm verify` meldet Abweichungen vom Profil | Bloßes Öffnen des QoS-Devices ohne Write setzt keine enge Anforderung. TuneD ist nach aktuellem Stand **kein** belegter C-State-Limiter; Abweichungen stammen plausibel auch von vorherigem PowerTOP-Autotune und sind separat aufzulösen |
| 2026-08-05 22:49 | `react-drm`/Touch-Bar Source und Runtime | Userdienst seit ~10 h aktiv | ~6:13 min CPU-Zeit (~1 % eines Cores im Langzeitmittel), >1 Mio. Context Switches; hält `/dev/dri/card0` (t2bdrm), Input-FDs und uinput offen. Systembar macht alle 2 s einen vollen Render+Blit; Clock 1/s; CAVA ist auf 24 fps konfiguriert, lief im Snapshot aber nicht als Kindprozess | Touch-Bar-Dienst ist ein gut isolierbarer Verstärker. Jeder DRM-Damage-Commit erzeugt synchrone USB-Bulk-Transfers zum T2; Treiber besitzt keine Runtime-PM-Callbacks. A/B-Test ist nötig, bevor er als Root Cause gilt |
| 2026-08-05 22:51 | PowerTOP CSV, 20 s, ohne Autotune | Aktive Desktop-/Codex-/Ptyxis-/Firefox-Sitzung; Messreport nur unter `/tmp` | 1.342 Wakeups/s, 40,5 % CPU-Usage; PC2 10,1 %, PC6 0,2 %, PC7–PC10 0. Top: `tick_nohz_handler` 762/s, sched-softirq 111/s, Firefox 74/s, `intel_atomic_commit_work` 11,3/s bei 142 ms CPU/s, Ptyxis 57 ms/s. `react-drm` 5,3/s, Fan 3,9/s, BCE-IRQ-Thread 3,1/s | Der aktuell beobachtete Zustand ist eindeutig workload-/observer-dominiert. T2-Pfade sind sichtbar, aber in diesem Fenster nicht die größten Wakeup-Quellen |
| 2026-08-05 22:52 | `turbostat`, 20 s Gegenprobe | Weiterhin interaktive Sitzung | Busy 17,61 %, `Any%C0` 66,03 %, PC2 6,02 %, PC6 0,17 %, PC7–PC10 0 %, 7.114 IRQ/s, Package 5,57 W | Nur ~34 % des Fensters war überhaupt kein Logical-CPU-Thread in C0. Zusammenhängende, lange All-Core-Idle-Fenster fehlen; tiefe Package-States sind unter dieser Last nicht zu erwarten |
| 2026-08-05 22:55 | ACPI-DSDT/GPE6E-Zuordnung | Aktive, vom Kernel verwendete DSDT nach `/tmp` kopiert und dekompiliert | `\_SB.PCI0.LPCB.EC` ist ein `PNP0C09` Embedded Controller mit `_GPE = 0x6E`; passende EC-Query-Methoden behandeln u. a. Batterie/SMBus, Lid, Netzteil und Performance-Notifications. Separates 20-s-Fenster: 165 Events = 8,25/s | GPE6E ist kein unbekanntes Gerät, sondern der normale ACPI-EC-Transport. Die Rate kann durch EC-Zugriffe/Polling mitverursacht sein; sie ist ein möglicher Verstärker, aber kein belegter Interruptsturm oder alleiniger PC8-Blocker |
| 2026-08-05 22:57 | Leise Display-on-Baseline, 45 s | Gesamte Skriptausgabe während des Fensters nach `/tmp` umgeleitet; keine PowerTOP-UI oder Terminalausgabe, normale GNOME-Sitzung/Apps blieben aktiv | Busy 11,63 %, `Any%C0` 58,59 %, PC2 8,76 %, PC3 0,01 %, PC6 0,21 %, PC7–PC10 0 %, Package 4,68 W, ~4.615 IRQ/s. i915 348/s, BCE-DMA 40/s, NVMe 39/s, WLAN 22/s, EC 7,4/s; kein DC5→DC6 | Weniger Diagnose-Observer-Effekt senkt Package-Leistung und BCE-IRQ-Rate, löst das Problem aber nicht. Auch diese Sitzung ist mit durchschnittlich ~11,6 % Busy über 8 Threads und `Any%C0` 58,6 % noch weit von echtem Idle entfernt |
| 2026-08-05 23:06 | Lock/Display-off-Baseline, 120 s | Andere Programme geschlossen; Skript mit 15 s Settle gestartet, anschließend per Cmd+L gesperrt und Bildschirm aus | Busy 5,56 %, `Any%C0` 31,02 %, Core C7 77,21 %, GFX RC6 99,79 %; PC2 38,05 %, PC3 0,06 %, PC6 1,07 %, PC7 0,40 %, PC8 ~0,0003 %, PC10 ~0,0010 %; Package 2,25 W. IRQ/s: NVMe 64,2, BCE 30,9, WLAN 11,0, i915 6,1, EC 5,5 | Display-off halbiert Package-Leistung und beseitigt fast alle i915-IRQs, aber PC2 dominiert weiter. PC8/PC10 wurden für Mikrosekunden erreicht: Hardware/Kernel erlauben sie grundsätzlich. Verbleibende CPU-/Device-Aktivität ist der nächste Fokus |
| 2026-08-05 23:11 | Intel-PMC LTR und PCIe-Portzuordnung | Nach Entsperren, Geräte unverändert | `SOUTHPORT_A`: 61.440 ns; `SOUTHPORT_B`: 2.097.152 ns. Intel-Portschema und `lspci`: SPA = Root Ports 1–4, hier `00:1c.0` → BCM4364 WLAN; SPB = Ports 5–8, hier `00:1c.4` → T2/ANS2. WLAN-Link hat L1.2 aktiv, Endpoint/Port D0; T2-Link nur L1.1 | Der enge SPA/WLAN-LTR ist jetzt der stärkste einzelne Device-Latency-Kandidat für PC2-Demotion. Noch per kontrolliertem WLAN-off- oder LTR-ignore-A/B-Test zu beweisen; `ltr_ignore` nicht ohne separaten Sicherheitstest persistieren |
| 2026-08-05 23:13 | Installationshistorie der T2-Userspace-Dienste | Read-only Journal-/Dateimetadaten | `react-drm.service` lief bereits am 16. Juli unmittelbar in den ersten dokumentierten Installationsboots; aktuelles Build und Fan-Binary wurden am 3. August aktualisiert | Existenz von `react-drm` allein erklärt den Unterschied zur frischen Installation nicht. Änderungen am Build oder dessen aktueller UI können trotzdem relevant sein |
| 2026-08-05 23:16 | WLAN/LTR Kurz-A/B, 10 s offline | Nur NetworkManager-WLAN-Radio reversibel aus/an; keine PCI-, LTR-Ignore- oder Moduländerung; Trap stellte WLAN wieder her | WLAN an: SPA 61.440 ns, SPB 143.360 ns. Nach 3 s WLAN aus: SPA 3.145.728 ns, SPB 2.097.152 ns; nach 10 s unverändert. 12 s nach WLAN an: SPA wieder 61.440 ns, SPB 143.360 ns. BCM4364 blieb durchgehend D0, `power/control=on` | **Starke direkte Korrelation:** Der aktive Broadcom-WLAN-Zustand verengt SPA um Faktor 51. Das geschieht unabhängig von PCI-Runtime-Suspend und ist ein sehr plausibler PC2-Demotionsgrund. C-State-A/B mit WLAN off ist für Kausalbeweis erforderlich |
| 2026-08-05 23:19 | WLAN-off plus Lock/Display-off, 120 s | WLAN per Trap aus/an; Skript mit 15 s Settle; Restore-Check: WLAN danach `enabled`, Link `UP` | Gegenüber WLAN-on/Lock: PC2 38,05→13,58 %, PC7 0,40→7,15 %, PC8 ~0,0017 %, PC10 ~0,007 %, DC5→DC6 0→10; WLAN-IRQs 11/s→0. SPA 3,146 ms, SPB 2,097 ms; SPA war power-gated. Messung zugleich stärker belastet: Busy 5,56→7,94 %, `Any%C0` 31,02→41,14 %, Package 2,25→3,08 W, i915 6→131 IRQ/s | **Kausalität bestätigt:** Trotz schlechterer CPU-/Display-Bedingungen sinkt PC2 um 24,5 Prozentpunkte und PC7 steigt ~18-fach. Aktives Broadcom-WLAN/LTR ist ein realer Hauptblocker. PC8 bleibt wegen zusätzlicher Last oder mindestens eines weiteren Plattformblockers praktisch null |
| 2026-08-05 23:19 | Prozesslast im WLAN-off-Lock-Fenster | Erstmals `pidstat` parallel im Skript | Ptyxis 17,81 %, Firefox 10,72 %, GNOME Shell 7,38 %, Codex 2,70 %, mehrere Firefox-Contentprozesse 1–2 %, `react-drm` 1,25 % eines Cores | Offline-Reconnects/Codex-UI und Desktoparbeit kontaminieren den A/B-Lauf. Der WLAN-Effekt ist deshalb eher unterschätzt; für den zweiten Blocker braucht es Online-Lock-Prozessbaseline oder Messung nach Ende der interaktiven Codex-Sitzung |
| 2026-08-05 23:24 | WLAN-Powersave-Status nach Restore | WLAN wieder verbunden (`Fischer-Netz 1`) | Kernel/`iw`: `Power save: on`; NetworkManager-Profil: `802-11-wireless.powersave=default` | Fehlender WLAN-Powersave ist nicht die Ursache. Der aktive BCM4364-Link fordert trotz aktivem Powersave nur 61,44 us SPA-Latenz an; PowerTOPs WLAN-Powersave-Tunable kann diesen LTR-Befund daher nicht lösen |
| 2026-08-05 23:29 | Historische Rekonstruktion des ersten PowerTOP-Laufs | Persistente Journale seit Installation | PowerTOP wurde erst am 17. Juli 10:59 installiert und gestartet: Boot -42, Kernel 7.1.3, WLAN seit 10:32 verbunden, `react-drm` und `t2-fancontrol` aktiv. Die T2-Module waren ebenfalls geladen. Der Lauf war 27 min nach einem S3-Resume | Kernel 6.19, fehlendes WLAN und bloße Existenz der KaiT2en-Dienste erklären die historischen ~50 % PC8 nicht. 7.1.3 versus 7.1.6, geänderte Dienstversionen und tatsächliche Idle-Last bleiben Deltas; das alte PowerTOP-Ergebnis selbst wurde nicht gespeichert |
| 2026-08-05 23:34 | SPA-LTR/Traffic-Kurzzeitreihe, WLAN verbunden | Display an, 20 s keine weitere Eingabe | SPA blieb in allen 20 Samples bei 61.440 ns. Selbst zwei aufeinanderfolgende Sekunden ohne RX- und TX-Paketzuwachs relaxierten den Wert nicht | 61,44 us ist kein nur bei einzelnen Paketen kurz gesetzter Boost. Längere echte Funkruhe im Lock-Zustand noch messen; Endpoint-LTR-Register selbst ist ungültig/0 ns, der enge Wert erscheint im PCH/PMC-Aggregat |
| 2026-08-05 23:37 | Online-120-s-Lauf mit erweiterter Messung | Lock war nicht über das ganze Fenster wirksam bzw. Displaypfad wurde geweckt; Ergebnisdatei `/tmp/cstates-online-locked-v2.txt` | Busy 10,25 %, `Any%C0` 53,33 %, GFX RC6 73,93 %, i915 346 IRQ/s; PC2 10,64 %, PC7 0,32 %, PC8–10 0. Ptyxis 25,32 %, GNOME 14,52 %, Firefox 8,94 %, Codex 4,71 % eines Cores. WLAN ~12,3 RX-/10,5 TX-Pakete/s. SPA am Ende 61.440 ns | **Keine gültige Lock-/Idle-Baseline.** Die Werte belegen erneut aktive Display-/Workload-Dominanz, dürfen aber nicht mit dem ruhigen ersten Lock-Lauf oder WLAN-off-Lauf kausal verglichen werden. LTR-Zeitreihensummary hatte einen Parserfehler; Parser danach korrigiert |
| 2026-08-05 23:43 | Kernel-Source-Review: brcmfmac-PM und PMC-LTR | Offizielle Upstream-Quellen plus Symbolprüfung des laufenden Moduls; keine Writes | `brcmfmac` setzt bei `iw power_save on` lediglich Firmwaremodus `PM_FAST` via `BRCMF_C_SET_PM` und `pm2_sleep_ret`; im PCIe-Treiber existieren weder LTR-Programmierung noch Runtime-PM-Callbacks. Der laufende `intel_pmc_core` bietet getrennt `ltr_ignore` und `ltr_restore`; Ignore setzt das Bit, Restore löscht es wieder | Der enge SPA-Wert stammt sehr wahrscheinlich aus Broadcom-Firmware/Hardware bzw. PCH-Aggregation, nicht aus einem expliziten Linux-LTR-Wert. Ein index-0-Ignore-A/B ist technisch gezielt und reversibel, aber erst nach Maintainer-Freigabe ausführen |
| 2026-08-06 09:05 | Präzisierte historische Beobachtung des Maintainers | Erinnerung, nicht maschinell gespeicherter Messreport | Standalone/Akku, internes Display an und WLAN aktiv erreichte früher wiederholt ca. 20–30 % PC8. Der einmalige PC10-Zustand könnte dagegen an einer USB-C-Dockingstation mit deaktiviertem WLAN gewesen sein. Gerät kam vor der heutigen Diagnose aus ca. 8 h S3 | Dock/C10 passt zum bestätigten WLAN-Effekt. Wiederholtes PC8 mit WLAN beweist aber einen zusätzlichen heutigen Regressionseffekt. KaiT2en-/Cmdline-/Thunderbolt-Änderungen seit Juli gezielt historisch abgleichen |
| 2026-08-06 09:18, korrigiert 09:53 | Historische Thunderbolt-Rekonstruktion | Bootjournale aller 45 vorhandenen Boots plus späterer Review des offiziellen Kernelpfads | Beim ersten PowerTOP-Lauf am 17. Juli meldeten beide ICL-NHIs `device links to tunneled native ports are missing!`; kein `t2thunderbolt`. Das Modul wurde am 22. Juli eingeführt und ist erstmals in Boot -28 am 23. Juli mit 8 PM-Links dokumentiert. **Korrektur:** Die Warnung bricht den generischen Probe nicht ab; `tb_cm_alloc()` gibt danach die Domain zurück. Sie zeigt fehlende von Kernel-ACPI/Apple-Helfern erzeugte Links, nicht einen fehlgeschlagenen NHI-Probe. Vorab von `t2thunderbolt` erzeugte Links werden von dieser Bool-Prüfung ebenfalls nicht erkannt, daher erscheint die Warnung auch beim heutigen Reload | Das sichere historische Delta sind die acht KaiT2en-PM-Links, nicht die Bindung des generischen Treibers. Ein Link-only-A/B mit weiterhin gebundenem `thunderbolt` bildet die Historie besser nach als ein kombinierter Unload. Offizieller Pfad: <https://github.com/torvalds/linux/blob/master/drivers/thunderbolt/tb.c#L3398-L3406> |
| 2026-08-06 09:21 | Geplanter Display-on-LTR-Ignore-A/B-Lauf | Firefox geschlossen, Akku, WLAN an, internes Display an; Maintainer ließ das Gerät ca. 4 min unberührt | **Ungültiger Versuch:** Der per `sudo -n nohup ... &` gestartete Prozess wurde beim Ende des gekapselten Codex-Startbefehls beendet. Keine Controller- oder Messdatei entstand; das leere Launcher-Log ist auf 09:21:22 datiert. Da der Controller nicht einmal seine erste Datei anlegte, wurde `ltr_ignore` nie beschrieben | Kein Messergebnis und keine Systemänderung. Wiederholung über eine offen gehaltene Exec-Session statt eines abgekoppelten Prozesses |
| 2026-08-06 09:28–09:31 | Display-on SOUTHPORT_A-LTR-Ignore A/B, je 90 s | Akku, WLAN verbunden, internes Display an; Firefox/Ptyxis geschlossen; LTR-Ignore nur im zweiten Fenster; Restore verifiziert (`LTR_IGNORE: 0`, `result=success`) | Normal → Ignore: Busy 0,85→0,83 %, `Any%C0` 5,62→5,60 %, GFX RC6 jeweils 99,99 %, i915 16→17 IRQ/90 s; PC2 49,30→61,36 %, PC6 28,61→0,33 %, PC7 1,10→3,15 %, **PC8 7,84→21,45 %**, PC9/10 0; Package 1,18→1,32 W. PC8-Rohdelta 7.066.488→19.340.182. WLAN-Traffic war im Ignore-Fenster geringer (RX/TX-Pakete 780/659→265/90; WLAN-IRQs 899→320). Der rohe SPA-LTR wechselte auch ohne Eingriff: normal 29/45 Samples 61,44 us, 16/45 3,146 ms; Ignore 23/45 bzw. 22/45 | **Gezielter LTR-Ignore reproduziert die historische Größenordnung von 20–30 % PC8 bei Display an, WLAN aktiv und heute funktionierendem Thunderbolt.** All-Core- und Grafik-Idle waren nahezu identisch, daher starke Kausalevidenz für SPA-LTR als PC8-Demoter. Wegen geringerem WLAN-Traffic und fester A→B-Reihenfolge noch reversen B→A-Kontrolllauf durchführen. PC10 wird durch SPA-Ignore allein nicht erreicht; Package-Watt sank in diesem Lauf nicht |
| 2026-08-06 09:36–09:40 | Reverser Display-on SOUTHPORT_A-LTR-Ignore B/A, je 90 s | Gleiche ruhige Akku-/WLAN-/Display-on-Bedingung; zuerst Ignore, dann Restore, 15 s Settle vor beiden Fenstern; Restore vor A und am Ende verifiziert (`LTR_IGNORE: 0`, `result=success`) | Ignore → Normal: Busy 0,91→0,78 %, `Any%C0` 6,33→5,48 %, GFX RC6 100,00→99,99 %, i915 jeweils 16 IRQ/90 s; PC2 58,81→48,52 %, PC6 0,33→29,66 %, PC7 3,40→0,97 %, **PC8 20,18→6,92 %**, PC9/10 0; Package 1,38→1,18 W. PC8-Rohdelta 18.196.134→6.235.475. Ignore hatte diesmal sogar mehr WLAN-Traffic (RX/TX-Pakete 438/343→254/115 normal), WLAN-IRQs (500→328) und NVMe-IRQs (7.209→5.694) | **Reihenfolge- und Traffic-Confound widerlegt:** Trotz messbar schlechterer Last-/Trafficbedingungen erreicht Ignore erneut ~20 % PC8, nach Restore fällt PC8 auf ~7 %. SPA-LTR ist damit als kausaler PC8-Demoter bestätigt. Der Effekt ist primär eine Umschichtung PC6→PC8; PC2 und Package-Watt werden dadurch nicht verbessert. Mindestens ein weiterer Faktor bestimmt die hohe PC2-Residency und PC10 bleibt blockiert |
| 2026-08-06 09:42 | Thunderbolt-Unload-Sicherheitsprüfung | Rein lesend; kein externes TB-/USB-C-Gerät angeschlossen | Thunderbolt-Sysfs enthält ausschließlich interne Host-Router `0-0`/`1-0` und `domain0`/`domain1`; keine Downstream-Geräte. Beide NHIs `00:0d.2/.3` an `thunderbolt`, D0/active. `thunderbolt` und `t2thunderbolt` jeweils Refcount/Used-by 0. Source: `t2thunderbolt_exit()` löscht alle erzeugten Device-Links; Ice-Lake-Pfad verändert keine D3-Flags | Kombinierter reversibler Unload ist vertretbar: erst generischen Treiber, dann Quirk/Links entfernen; Restore in Gegenreihenfolge und NHI-Bindings verifizieren. Test mit SPA-Ignore in beiden Fenstern, damit WLAN-LTR den TB-Effekt nicht maskiert |
| 2026-08-06 09:47–09:51 | Thunderbolt-Stack on/off A/B mit SPA-Ignore, je 90 s | Akku/WLAN/Display an, ruhig; SPA in beiden Fenstern ignoriert. B: erst `thunderbolt`, dann `t2thunderbolt` entfernt. Danach beide in Gegenreihenfolge geladen, NHI-Bindings und LTR-Restore verifiziert; `result=success` | On → Off: Busy 0,90→0,79 %, `Any%C0` 6,09→5,30 %, GFX RC6 ~100 %; PC2 59,82→42,26 %, PC6 0,32→0,38 %, PC7 3,26→3,31 %, **PC8 20,84→41,92 %**, PC9/10 0; Package 1,35→1,18 W. Ungebundene NHIs meldeten `runtime=active`, Power-State mangels Treiber `unknown`; nach Restore wieder `active/D0`. Off-Fenster hatte zugleich weniger NVMe-/WLAN-/i915-IRQs | **Starke Evidenz, dass der laufende Thunderbolt-Stack ein zweiter PC8-Blocker ist**, aber der kombinierte Unload entfernt generischen Treiber und KaiT2en-Links zugleich und hatte leicht günstigere Last. Da der generische Treiber historisch wahrscheinlich ebenfalls gebunden war, als Nächstes nur `t2thunderbolt`/acht Links entfernen und NHI-Bindings beibehalten. PC10 blieb selbst mit beiden bekannten Blockern entfernt/ignoriert 0 |
| 2026-08-06 09:56–10:00 | `t2thunderbolt`-Link-only A/B mit SPA-Ignore, je 90 s | Generischer `thunderbolt` und beide NHIs durchgehend gebunden/active/D0; B entfernt ausschließlich `t2thunderbolt` und dessen acht Links. Modul/Links und LTR danach wiederhergestellt; `result=success` | Links on → off: Busy 0,85→0,86 %, `Any%C0` 5,72→5,68 %, GFX RC6 100,00→99,99 %; PC2 60,89→61,09 %, PC6 0,32→0,32 %, PC7 3,28→3,12 %, **PC8 21,50→21,64 %**, PC9/10 0; Package 1,38→1,41 W. Alle Unterschiede innerhalb Messrauschen | **Die acht KaiT2en-PM-Links sind energetisch neutral und nicht der C-State-Regressionsverursacher.** Der große kombinierte Thunderbolt-off-Effekt stammt folglich vom generischen Treiber bzw. der durch ihn initialisierten NHI-Hardware. Als Bestätigung generischen Treiber allein in reverser Reihenfolge testen, während `t2thunderbolt` geladen bleibt |
| 2026-08-06 10:02 | Thunderbolt-Runtime-PM- und Versionsreview | Rein lesend; lokales Sysfs und offizielle Stable-Quellen 7.1.3/7.1.6 | Beide NHIs: `control=auto`, Autosuspend 15 s, seit Boot `runtime_suspended_time=0`. Interne Host-Router `0-0`/`1-0`: `runtime_status=unsupported`. `tb_start()` setzt `root_switch->rpm` nur für USB4; Ice-Lake-TB3-Root-Router bekommt daher kein Runtime-PM. Relevanter `tb.c`-/`nhi.c`-Pfad ist in Stable 7.1.3 und 7.1.6 identisch. Lokal installiert: 7.1.5-200, 7.1.5-201, 7.1.6-201; 7.1.3 nicht mehr vorhanden | Dauer-D0 ist konsistent mit deaktiviertem Root-Switch-Runtime-PM und kein PowerTOP-Tunable. Eine einfache 7.1.3→7.1.6-Patchlevel-Regression in diesem Upstream-Pfad ist unwahrscheinlich. Ob Ice-Lake-TB3 sicher Runtime-PM unterstützen kann, wäre eine separate Kernel-Entwicklungsfrage; zunächst Generic-only-Effekt replizieren. Quellen: <https://github.com/gregkh/linux/blob/v7.1.6/drivers/thunderbolt/tb.c#L2988-L3007>, <https://github.com/gregkh/linux/blob/v7.1.6/drivers/thunderbolt/nhi.c#L1416-L1421> |
| 2026-08-06 10:06–10:09 | Reverser Generic-`thunderbolt`-only B/A mit SPA-Ignore, je 45 s | `t2thunderbolt`/acht Links durchgehend geladen. Erst nur generischen Treiber entfernt, dann geladen und beide NHIs wieder gebunden; 15 s Settle je Fenster. LTR und Bindings am Ende verifiziert; `result=success` | Generic off → on: Busy 1,03→0,95 %, `Any%C0` 6,86→6,21 %, GFX RC6 jeweils 99,99 %; **PC2 40,80→60,49 %**, PC6 0,39→0,33 %, PC7 3,60→3,13 %, **PC8 38,15→21,46 %**, PC9/10 0; Package 1,42→1,43 W. Off hatte trotz besserer C-States mehr NVMe-IRQs (4.627→3.103), höhere Core-Leistung (0,41→0,33 W) und mehr CPU-Last | **Direkter, reverser Kausalbeweis:** Der generische Thunderbolt-Treiber bzw. der von ihm aktive NHI/Host-Router-Zustand halbiert PC8 ungefähr und demotet ~20 Prozentpunkte nach PC2. Höhere Off-Last schließt einen günstigen Workloadtrend als Erklärung aus. Der Effekt ist vom neutral getesteten `t2thunderbolt`-Linkmodul getrennt. Kein messbarer Package-Watt-Effekt in diesem kurzen Lauf; PC10 bleibt anderweitig blockiert |
| 2026-08-06 20:40–20:44 | Kompletter Thunderbolt-Stack an/aus bei normalem WLAN-LTR, je 90 s | Akku, WLAN verbunden, internes Display an; LTR durchgehend normal (`LTR_IGNORE: 0`, SPA überwiegend 61,44 us). Stack nach B vollständig wiederhergestellt; beide NHIs wieder `thunderbolt`, `auto`, `active/D0`; `result=success` | **Als Idle-A/B ungültig:** `Any%C0` war in beiden Fenstern 40,50 %, GFX RC6 nur 74,50→74,87 %, i915 ca. 27.300 IRQ/90 s. Ptyxis belegte 21,56→19,98 %, GNOME Shell je ~11 % und Codex ~3 % eines Cores. Entsprechend PC8 nur 0,66→1,08 %, PC2 21,03→20,70 %, Package 3,22→3,28 W. Das Off-Fenster hatte zudem 19.029 statt 4.223 NVMe-IRQs | Der sichtbare Codex/Ptyxis-Desktop maskierte alle Device-Effekte; aus PC8 0,66→1,08 % folgt **keine** Widerlegung des Thunderbolt-Befunds. Realwelt-Test mit normalem LTR wiederholen, nachdem Ptyxis nach Start geschlossen/minimiert wurde. Kein Zustand blieb verändert |
| 2026-08-06 20:46–20:50 | Gültige Wiederholung: kompletter Thunderbolt-Stack an/aus bei normalem WLAN-LTR, je 90 s | Nach Start wurde Ptyxis geschlossen; Akku, WLAN verbunden, internes Display an. LTR durchgehend normal (`LTR_IGNORE: 0`); Restore vollständig verifiziert (`result=success`) | On → off: Busy 0,78→0,76 %, `Any%C0` 5,38→5,18 %, GFX RC6 100,00→99,99 %, i915 5→16 IRQ/90 s; **PC2 48,90→42,70 %**, PC6 29,64→34,36 %, PC7 1,02→0,74 %, **PC8 7,26→9,80 %**, PC9/10 0; Package 1,21→1,13 W, Core 0,22→0,21 W. Off hatte sogar öfter den engen SPA-LTR (35/45 statt 31/45 Samples bei 61,44 us); CPU-/Trafficbedingungen waren ansonsten hinreichend ähnlich | **Praxisantwort:** Thunderbolt komplett aus hilft bei unverändertem WLAN nur moderat (+2,54 Prozentpunkte PC8, −6,20 pp PC2, im Einzellauf −0,08 W) und erreicht nicht die gewünschten 20–30 % PC8. Der große TB-off-Effekt bei SPA-Ignore ist eine Interaktion beider Demoter. TB-off kann persönliche/optionale Optimierung sein, ersetzt aber keine WLAN-/LTR-Lösung |
| 2026-08-06 20:55 | Historische BCM4364-Firmware-Rekonstruktion | Alle 45 Bootjournale, lokale Firmware-Metadaten und KaiT2en-Git-Historie; keine Writes | Die heutigen Apple-Blobs wurden am 27. Juli neu installiert/benannt, aber bereits die Boots vom 16./17. Juli meldeten dieselbe Firmware `9.30.514.0.32.5.94`/FWID `01-68d7ff80` und luden den TxCap-Blob. Dieselbe Version läuft lückenlos von Kernel 6.19.10 bis 7.1.6. BCM4364-AER-Fehler `Rollover`/`Timeout` traten in einzelnen Boots auf 7.1.3, 7.1.4 und 7.1.5 auf, nicht an einer eindeutigen Versionsgrenze | Die Dateigeburt am 27. Juli ist **kein Beleg** für einen Firmware-Inhaltswechsel und erklärt die Regression wahrscheinlich nicht. Die AERs passen zu aggressiven PCIe-Schlafphasen, sind aber sporadisch und kein sauberer Regressionsmarker |
| 2026-08-06 21:00 | brcmfmac-/PCIe-Power-Review | Laufender Fedora-Kernel, vollständige PCIe-Capabilities und aktueller offizieller Upstream-Source; keine Writes | BCM4364 und Root Port haben ASPM L1 plus L1.1/L1.2 aktiv. Programmierter L1.2-Schwellenwert ist 90.112 ns; der dynamische SPA-LTR von 61.440 ns liegt darunter und kann L1.2 folgerichtig sperren. Endpoint-LTR-Konfigregister selbst zeigen 0 ns, der enge Wert kommt per dynamischer Hardware/Firmware-Aggregation. `Power save: on` setzt in brcmfmac ausschließlich Firmwaremodus `PM_FAST`; `PM_MAX` wird nur für WoWLAN/Suspend verwendet. PCIe-PM-Callbacks existieren nur für System-Suspend (`suspend/resume/freeze/restore`), nicht Runtime-PM | Kein vorhandener Modul-/NetworkManager-Schalter löst dies. Sinnvolle Reihenfolge: zuerst sehr kurz `PM_FAST` gegen `PM_OFF` auf LTR testen; danach als Kernelexperiment `PM_MAX` im verbundenen Betrieb. Runtime-D3 ist ein deutlich größeres Projekt. Globales SPA-Ignore bleibt wegen verletztem 61,44-us-Vertrag kein Produktions-Fix. Quellen: <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/broadcom/brcm80211/brcmfmac/cfg80211.c#L3097-L3143>, <https://github.com/torvalds/linux/blob/master/drivers/net/wireless/broadcom/brcm80211/brcmfmac/pcie.c#L2467-L2543> |
| 2026-08-06 21:02–21:03 | WLAN-Powersave/LTR-Kurz-A/B | Verbunden mit demselben AP; 15 s `PM_FAST`, 15 s `PM_OFF`, danach 10 s restored `PM_FAST`; 0,5-s-Samples. Transienter Dienst, Trap und Endprüfung; `result=success` | `PM_FAST`: 30/30 SPA-Samples 61.440 ns. `PM_OFF`: ebenfalls 30/30 bei 61.440 ns. Restored `PM_FAST`: 20/20 bei 61.440 ns. Interface blieb `up`, AP/BSSID unverändert, Linkrate vor/nach 400 MBit/s | Der enge SPA-LTR ist unabhängig vom normalen cfg80211-Powersave-Schalter. Power Save auszuschalten bietet keinen C-State-Weg und wäre energetisch kontraproduktiv. Zustand vollständig restored (`Power save: on`, `LTR_IGNORE: 0`) |
| 2026-08-06 21:13–21:18 | brcmfmac-`PM_MAX`-Testbuild vorbereitet, **nicht geladen** | Exaktes Fedora-SRPM `kernel-7.1.6-201.fc44.src.rpm` nach `/tmp` geladen; Fedora-Red-Hat-Patch berührt die betroffenen brcmfmac-Dateien nicht. Externer Build gegen laufendes `kernel-devel-7.1.6-201.fc44`; Secure Boot aus, Kernel bereits durch KaiT2en-Module `O/E`-getainted. Mac inzwischen an Dock: Ethernet `enp0s13f0u1u1` ist Default mit Metric 100, WLAN bleibt verbunden mit Metric 600 | Build erfolgreich; BTF mangels unkomprimiertem `vmlinux` übersprungen. `brcmfmac.ko` Vermagic exakt passend, Parameter `power_save_mode=1=PM_MAX, 2=PM_FAST(default)`, andere Werte werden abgelehnt; SHA-256 `0b0acf30932a5cc789103326889664b62ae1d9415d2eeb0da9ebbcd2199b0e5b`. Patch-Dry-Run, Syntax und ShellCheck erfolgreich. Der danach ausgeführte read-only `--preflight` bestand sämtliche Guards für Kernel, Modulzustand, normales LTR, WLAN-Assoziation und kabelgebundene IPv4-Default-Route. Untracked Paket: `c-states-brcmfmac-pm-max-testbuild/` mit Binärmodul, Patch, automatischem LTR-Test/Stock-Restore und separatem Restore-Helfer | Laufendes System blieb unverändert: Stock-`brcmfmac`, kein neuer Parameter, WLAN verbunden/Power Save an. Nächster aktiver Test darf erst nach Vorbereitung/`bereit` als transienter Dienst laufen; währenddessen nicht suspendieren oder undocken. Zuerst nur LTR/Verbindung prüfen, noch kein C-State-Fenster |
| 2026-08-06 21:23–21:26 | Erster aktiver `PM_MAX`-LTR-Versuch als transienter Dienst | Stock-Baseline vollständig: 20/20 SPA-Samples bei 61.440 ns. Danach WLAN aus und Stock-Module entladen; direktes `insmod` des untracked Moduls aus dem Workspace scheiterte mit `EPERM`. EXIT-Trap lud Stock-`brcmfmac`/`brcmfmac_wcc`, aktivierte WLAN und stellte die Assoziation wieder her | Kein `PM_MAX`-Messwert, also **kein** Ergebnis zur Hypothese. Eindeutiger AVC: `kmod_t` durfte `module_load` auf `user_home_t` nicht ausführen. Secure Boot aus, Lockdown `none`, `modules_disabled=0`; damit ist SELinux die alleinige Ursache der Ablehnung. Laufend wieder Fedora-Stock-Modul, Testparameter abwesend. Testhelfer danach geändert: private temporäre Kopie, nur diese erhält `modules_object_t`, automatische Löschung bei Erfolg/Fehler; Label-Probelauf, Syntax und ShellCheck erfolgreich | Sicherheitsmechanismus verhielt sich korrekt. Der nächste aktive Versuch benötigt erneut Vorbereitung/`bereit`; die fehlgeschlagene Messung darf nicht als PM_MAX-Datenpunkt gewertet werden |
| 2026-08-06 21:27–21:28 | Erfolgreicher verbundener WLAN-LTR-Test: Stock-`PM_FAST` → Testbuild-`PM_MAX` → Stock-`PM_FAST` | Mac am Dock, Ethernet Default-Route Metric 100, WLAN mit demselben AP verbunden Metric 600. Transienter Dienst; temporäre Modulkopie korrekt `modules_object_t`; Testparameter während B verifiziert `1`; vollständiger automatischer Restore | Stock A: 20/20 SPA-Samples **61.440 ns**. `PM_MAX`: 30/30 Samples **3.145.728 ns**, WLAN blieb am selben BSSID assoziiert. Restored Stock: 10/10 Samples wieder **61.440 ns**. Dienst `result=success`; laufend wieder paketiertes Fedora-Modul, Testparameter abwesend, WLAN verbunden und Power Save an | **Hypothese bestätigt:** Nicht „WLAN verbunden“ an sich, sondern brcmfmac-Firmwaremodus `PM_FAST` erzeugt den für L1.2 zu engen LTR. `PM_MAX` liefert im aktiven verbundenen Betrieb denselben entspannten SPA-LTR wie WLAN-off, ohne globales `LTR_IGNORE`. Als Nächstes 45-s-C-State-A/B unter ruhigem Desktop; danach erst Netzwerkqualität, Roaming und S3 bewerten |
| 2026-08-06 21:31–21:34 | Erster 45-s-C-State-A/B Stock-`PM_FAST` → `PM_MAX`, gedockt/AC | Transienter Dienst mit 12 s Start-Grace und 5 s Settle je Fenster; Dock/Ethernet blieb aktiv. Vollständiger Restore mit `result=success`; Testmodul und temporäre Kopie entfernt, Fedora-Stock-Modul/WLAN-Powersave/Assoziation wiederhergestellt | **Als C-State-A/B ungültig.** Stock bereits unruhig: Busy 3,03 %, `Any%C0` 23,02 %, GFX-RC6 99,98 %, PC2 31,59 %, PC6 0,26 %, PC8 0; XHCI `00:14.0` 45.254 IRQ/45 s (~1.006/s), WirePlumber 4,78 % plus alsactl 2,55 % eines Cores. Während des zweiten Fensters kam die interaktive Rückkehr: `PM_MAX` Busy 5,93 %, `Any%C0` 37,40 %, GFX-RC6 95,17 %, PC2 21,30 %, PC6 0,11 %, PC8 0; Ptyxis 10,64 %, Codex 4,33 %, GNOME 4,02 %, i915 4.682 IRQ. Der LTR-Teil blieb gültig: Stock-Timeline 10/23 bei 61.440 ns und 13/23 bei 3.145.728 ns; `PM_MAX` 23/23 bei 3.145.728 ns | Null PC8 widerlegt `PM_MAX` nicht: Beide Fenster verfehlten die bekannte notwendige Ruhe (`Any%C0` ~5–6 %, GFX-RC6 ~100 %), B zusätzlich klar durch Rückkehr kontaminiert. Dock-/USB-XHCI ist hier ein neuer großer Wakeup-Faktor. Nächster C-State-Versuch sinnvollerweise standalone auf Akku, als unabhängiger Dienst trotz kurzem WLAN-/Codex-Verbindungsabbruch; großzügigere Rückkehrfrist verwenden |
| 2026-08-06 21:38–21:40 | Standalone/Akku-A/B-Versuch; gültiges Stock-Fenster, kontrollierter Abbruch vor `PM_MAX`-Fenster | Alle Standalone-Guards bestanden: Akku, WLAN einzige Default-Route, Display an, ruhiger Desktop. 45 s Stock gemessen. Testmodul danach erfolgreich mit Parameter `1` geladen und WLAN assoziiert; Controller brach wegen eines unmittelbar danach einmalig engen LTR-Samples ab und restaurierte Stock vollständig | **Gültiges Stock-`PM_FAST`-Fenster:** Busy 0,73 %, `Any%C0` 5,04 %, GFX-RC6 99,98 %, PC2 47,97 %, PC6 32,38 %, PC7 0,88 %, PC8 6,48 %, PC9/10 0, Package 1,32 W; SPA 17/23 Samples 61.440 ns und 6/23 bei 3.145.728 ns. XHCI-IRQs beide 0, i915 nur 16/45 s. `PM_MAX`-C-State-Fenster lief nicht; kein Vergleichswert. Stock-Datei zusätzlich als `/tmp/cstates-brcmfmac-pm-fast-stock-standalone-valid.txt` erhalten, SHA-256 `db64b572abe55c83a323287d16cc1fe0a6d1f80c322456fea1abc574e5f4f399` | Guard war zu streng/racy: Reconnect-Traffic darf den dynamischen LTR kurz auf 61,44 µs ziehen; maßgeblich ist die Timeline nach Settle. Guard entfernen und nur ein `PM_MAX`-Fenster messen. Der Standalone-Stockwert repliziert die früheren 6,9–7,8 % PC8 sehr gut und quantifiziert den großen Dock-Wakeup-Effekt |
| 2026-08-06 21:43–21:45 | Gültiges ergänzendes 45-s-Standalone-`PM_MAX`-Fenster gegen den direkt zuvor gesicherten Stock-Wert | Akku, WLAN einzige Default-Route, Display an, ruhiger Desktop; nur fehlendes B-Fenster ausgeführt. Testparameter `1`, WLAN am selben AP verbunden; 23/23 SPA-Samples bei 3.145.728 ns. Dienst und Stock-Restore `result=success`; temporäres Modul gelöscht | Stock `PM_FAST` → `PM_MAX`: Busy 0,73→1,11 %, `Any%C0` 5,04→7,11 %, GFX-RC6 99,98→99,98 %; PC2 47,97→58,93 %, PC6 32,38→0,87 %, PC7 0,88→3,29 %, **PC8 6,48→20,36 % (+13,88 pp)**, PC9/10 0. Package 1,32→1,55 W. B war ungünstiger belastet: WLAN 1.203 statt 225 IRQ, ~1,02 MB statt ~32 KB Traffic, NVMe 4.318 statt 2.969 IRQ; dennoch verdreifachte sich PC8. Dateien `/tmp/cstates-brcmfmac-pm-{fast-stock,max}-standalone-valid.txt`, SHA-256 PM_MAX `7c9a8ae7367e9981fc81b405a0da2c2374e7b4854582860a3e22b833f922c0d9` | **Kausaler Praxisbeweis und Ziel erreicht:** brcmfmac `PM_FAST`/sein enger dynamischer SPA-LTR ist der wesentliche WLAN-PC8-Demoter; verbundenes `PM_MAX` erreicht trotz höherer Gegenlast 20–30-%-Zielbereich. Watt-Differenz ist wegen Last-/Traffic-Mismatch nicht als Mehrverbrauch interpretierbar. Vor Produktion Ping/Loss/Latenz, Durchsatz, Roaming/Reconnect, S3/WoWLAN und einen lastgematchten längeren Leistungs-A/B testen |
| 2026-08-06 22:01 | DKMS-Integration vorbereitet, noch **nicht im System installiert** | Neuer Branch `feat/brcmfmac-pm-max-dkms`. Fedora/Linux-7.1.6-`brcmfmac`-Quellsnapshot ohne Buildartefakte unter `modules/brcmfmac_kait2en/`; DKMS-Paket `brcmfmac_kait2en/7.1.6_kait2en1` ersetzt nur `brcmfmac.ko`, Fedora-`brcmutil` und `brcmfmac_wcc` bleiben Stock. `PM_MAX` ist Default im Testmodul; keine gefährliche persistente `modprobe.d`-Option. Regulärer Installer und DKMS-Kernel-Cleanup kennen das Paket; separates `restore-stock-brcmfmac.sh` entfernt DKMS-/Quell-/Hook-/eventuelle Initramfs-Reste und kann optional live reloaden | Isolierte Builds in `/tmp` gegen alle installierten Header `7.1.5-200`, `7.1.5-201`, `7.1.6-201` erfolgreich; Vermagic jeweils exakt, Modulmetadatum `kait2en_pm_default=PM_MAX`; 295 undefinierte Symbole exakt gleich dem erfolgreichen Testbuild. Shell-Syntax, ShellCheck und `git diff --check` sauber | Laufendes System weiterhin unverändert mit Fedora-Stock-`brcmfmac`. Nächster Schritt ist ein gezielter DKMS-Install/Resolve/Restore-Test **ohne** Modulreload; vorher erneut genaue Vorbereitung und `bereit` abwarten |

| 2026-08-06 22:06 | Parallel getesteten `t2bdrm`-Stand in den neuen Branch übernommen | Maintainer hat systemweit derzeit Branch `fix/t2bdrm-bounded-requests` installiert. Commit `e3a174f` berührt ausschließlich `modules/t2bdrm/t2bdrm.c` und basiert direkt auf demselben Main-Stand; sauber nach `feat/brcmfmac-pm-max-dkms` cherry-picked als `749f6ca` | Isolierter `t2bdrm`-Build gegen `7.1.6-201.fc44` erfolgreich, Vermagic exakt; vollständige Installer-Tests erneut erfolgreich | Der normale DKMS-Installer kann nun für den Lifecycle-Test verwendet werden, ohne den parallel getesteten bounded-requests-Treiber auf Main/Stock zurückzusetzen |

| 2026-08-06 22:07–22:12 | Vollständiger DKMS-Install-/Resolve-/Restore-Lifecycle über den regulären KaiT2en-Weg | `scripts/fedora/install-dkms-modules.sh` baute und installierte alle Branch-Module erfolgreich. `brcmfmac_kait2en/7.1.6_kait2en1` wurde von DKMS signiert und als `/lib/modules/7.1.6-201.fc44.x86_64/extra/brcmfmac.ko.xz` bevorzugt; `modinfo` zeigte passende Vermagic, `kait2en_pm_default=PM_MAX` und den Parameter. Gleichzeitig blieb im laufenden Kernel der Stock-Treiber aktiv (Sysfs-Parameter abwesend), daher kein WLAN-Reload. Installierter `t2bdrm`-Source enthielt den bounded-requests-Commit. Danach neues Restore-Skript ohne `--reload` ausgeführt | DKMS restaurierte das archivierte Fedora-Original; RPM-Verifikation sauber, Stock-SHA-256 `ba1abdfb2ac10f57babd1c4f3aec9f6b726de438de35b24a3c3fe786a5d61603`. Kein Paket in DKMS, kein `/var/lib/dkms/brcmfmac_kait2en`, kein `/usr/src/brcmfmac_kait2en-*`, kein `/extra/brcmfmac.ko*`, kein Kernel-Hook-Eintrag, kein temporärer DKMS-Override und keine Testkopie im Initramfs. `modinfo -n brcmfmac` zeigt wieder Fedora `/kernel/.../brcmfmac.ko.xz`; laufender Parameter weiterhin abwesend. WLAN `enabled` und mit `Fischer-Netz 1` verbunden. Gewünschter `t2bdrm/0.1` bleibt installiert. Restore anschließend im sauberen Zustand idempotent erfolgreich | **Integration und Rückweg praktisch bestätigt.** Fedora DKMS 3.x nutzt hier `/extra` trotz deklarierter `DEST_MODULE_LOCATION`; Restore erkennt deshalb `/extra` und `/updates/dkms` in Initramfs. Der unnötige DKMS-Post-Transaction-Lauf wird nun wie im Installer temporär unterdrückt. System ist vollständig im WLAN-Stock-Zustand und bereit für eine spätere bewusste Alltagsinstallation plus Reboot |

| 2026-08-06 22:15–22:23 | Feature-Branch veröffentlicht und bewusst für den Alltagsversuch installiert; Reboot noch ausstehend | `feat/brcmfmac-pm-max-dkms` erfolgreich auf Fork `4f1sh3r`/`origin` gepusht. Vollständiges `scripts/fedora/install.sh` lief fehlerfrei durch: Dependencies, Kernel-Args, alle DKMS-Module, UCM/DSP, NetworkManager, Dienste, Plymouth, Initramfs, Suspend-Helfer und Apps. DKMS meldet `brcmfmac_kait2en/7.1.6_kait2en1` und `t2bdrm/0.1` installiert; nächste Modulauflösung jeweils `/lib/modules/7.1.6-201.fc44.x86_64/extra/*.ko.xz`; `brcmfmac`-Metadatum `PM_MAX`. `/usr/src/t2bdrm-0.1/t2bdrm.c` enthält `rows_per_request` und 128/64/32-KiB-Bounds. Neues Initramfs enthält keine vorgezogene `brcmfmac`-Kopie | Vor Reboot läuft erwartungsgemäß noch der zuvor geladene Fedora-Stock-`brcmfmac` (neuer Sysfs-Parameter abwesend). Der aktuell bereits geladene gepatchte `t2bdrm` lässt sich über die eindeutigen Kallsyms `appletbdrm_alloc_request` und `appletbdrm_send_frames` belegen. Installierte entpackte Modul-Fingerprints: brcmfmac SHA-256 `2b8f3c0a2ee4204cd7652dab74a7a04cd4b1fe56c58e177e7b513fb0e68ecc45`, t2bdrm `7f23ef4514400a3aae59e9236fc2a36e98ba7f6877b7575575edc05bb5da9238` | Maintainer rebootet manuell. Danach prüfen: Boot/Kernel, DKMS und Pfade, `power_save_mode=1`, `kait2en_pm_default=PM_MAX`, WLAN-Assoziation/Power-Save, entspannte SOUTHPORT_A-LTR-Zeitreihe sowie `t2bdrm`-Kallsyms. Erst danach ist der persistente Laufzustand bestätigt |

| 2026-08-06 22:24–22:26 | Erster persistenter Boot mit DKMS-`PM_MAX` und bounded-requests-`t2bdrm` | Kernel `7.1.6-201.fc44`, neuer Boot-ID `9c730b9f-a02a-454d-b256-600c243c84ac`. DKMS meldet beide Pakete installiert; `modinfo` löst beide nach `/lib/modules/.../extra/*.ko.xz` auf. Tatsächlich geladen: `brcmfmac`, `brcmfmac_wcc`, `brcmutil`, `t2bdrm`; brcmfmac-Sysfs `power_save_mode=1`, Modulmetadatum `PM_MAX`, BCM4364 an `e5:00.0` gebunden. WLAN `Fischer-Netz 1` auf `wlp229s0`, 5 GHz Kanal 36, Power Save an. Geladener `t2bdrm` enthält in `/proc/kallsyms` die patch-exklusiven Funktionen `appletbdrm_alloc_request` und `appletbdrm_send_frames`; `react-drm.service` aktiv/running | SOUTHPORT_A 12/12 Ein-Sekunden-Samples bei **3.145.728 ns** für Snoop und Non-Snoop, jeweils `LTR_IGNORE=0`. Installierte entpackte Modul-Hashes stimmen mit Pre-Reboot überein: brcmfmac `2b8f3c0a...ecc45`, t2bdrm `7f23ef45...9238`. Bootlog zeigt normalen Firmware-Probe; einzig `brcmf_p2p_set_firmware ... ret -52`, ohne Auswirkung auf Managed-WLAN-Verbindung | **Persistente Zielkonfiguration vollständig bestätigt:** gepatchter WLAN-Treiber läuft wirklich mit PM_MAX und gewünschtem vertragstreuem LTR; gepatchter bounded-requests-`t2bdrm` läuft weiterhin und wird von react-drm genutzt. Jetzt beginnt der Alltags-/Nebenwirkungstest; noch keine Aussage über reale Akkulaufzeit oder WLAN-Latenz |

| 2026-08-06, nach WLAN-Alltagstest | Persönliche persistente Thunderbolt-Deaktivierung vorbereitet, **noch nicht angewendet** | Die zunächst beobachtete hohe WLAN-Latenz kam vom stark sendenden Hausnetz; danach wieder normale 1–3 ms lokal und bislang keine erkennbare `PM_MAX`-Nebenwirkung. Root-owned, untracked Helfer `/usr/local/bin/disable-thunderbolt.sh` und `enable-thunderbolt.sh` installiert; Syntax und ShellCheck sauber. Disable blockiert `thunderbolt` und `t2thunderbolt` per modprobe-Regel, hält beide via Dracut-`omit_drivers` auch aus aktuellen/zukünftigen Initramfs und entlädt in getesteter Reihenfolge. Enable entfernt nur eigens markierte Dateien, regeneriert Initramfs und lädt `t2thunderbolt` vor `thunderbolt`. Schutzprüfungen für MBP16,2-NHIs, externe TB-Geräte und USB-C-xHCI enthalten | Noch keine Disable-Regel vorhanden; beide TB-Module weiterhin geladen, NHIs `00:0d.2/.3` an `thunderbolt`, USB-C-xHCI `00:0d.0` an `xhci_hcd`. Vor tatsächlicher Ausführung alle USB-C-/TB-Geräte trennen und Maintainer erneut mit `bereit` bestätigen lassen | Die persönliche Optimierung ist reversibel und kernel-update-sicher vorbereitet. Erwartung mit bereits aktivem `PM_MAX`: möglicherweise Annäherung an den früheren kombinierten Effekt (~20→~40 % PC8), aber dieser genaue kombinierte Produktionszustand ist noch nicht gemessen; kurzer Test zeigte keine belastbare Watt-Aussage |

| 2026-08-06 23:06–23:09 | Persönliches Thunderbolt-off persistent angewendet | Standalone, keine externen USB-C-/TB-Geräte. Erster Skriptlauf schrieb die korrekten Regeln, aber `dracut --regenerate-all` scheiterte vor dem Live-Unload an bereits bestehenden kaputten `7.1.5`-Modulabhängigkeiten (`t2bce_core`/`t2bce_dma` fehlen). Er schrieb dabei beide alten 7.1.5-Initramfs neu; deren Modultrees enthielten die referenzierten t2bce-Dateien bereits nicht, und die neuen Images enthalten sie folglich ebenfalls nicht. Helfer daraufhin auf gezielten Rebuild des laufenden Kernels geändert; die persistente Dracut-Regel gilt trotzdem automatisch für künftige Kernel. Syntax/ShellCheck erneut sauber, root-owned Versionen installiert und Disable erfolgreich wiederholt | `thunderbolt` und `t2thunderbolt` nicht geladen; NHIs `00:0d.2/.3` ungebunden. Modprobe-Auflösung für beide ist `install /bin/false`; markierte modprobe-/Dracut-Regeln vorhanden. Aktuelles `7.1.6-201`-Initramfs um 23:08 neu gebaut und enthält keine beiden Module. USB-C-xHCI `00:0d.0` bleibt an `xhci_hcd`. WLAN weiterhin verbunden, `power_save_mode=1`; SOUTHPORT_A weiterhin vertragstreu 3.145.728 ns, `LTR_IGNORE=0` | **Zielzustand aktiv und verifiziert.** Reboot-/Kernel-Update-Persistenz ist konfiguriert. Physische USB-C-Funktion wurde mangels angeschlossenem Testgerät noch nicht praktisch getestet; Controller/Driver blieben jedoch unangetastet. Alte 7.1.5-Kernel sind wegen ihrer schon zuvor fehlenden DKMS-t2bce-Module kein verlässlicher Fallback und sollten nicht als solcher betrachtet werden. Nächster optionaler Schritt: ruhiges 45-s-PC-State-Fenster im kombinierten `PM_MAX`+TB-off-Zustand und später Reboot-Verifikation |

| 2026-08-06 23:18–23:19 | Erster persistenter `PM_MAX`+Thunderbolt-off-Messlauf, **ungültig als Idle-Vergleich** | Unabhängiger Dienst, 20 s Settle + 45 s. Die interaktive Startprüfung und das Schließen von Ptyxis lagen noch im Messfenster; laufender PowerTOP-Prozess ebenfalls erfasst | Busy 4,26 %, `Any%C0` 25,50 %, GFX-RC6 89,86 %, i915 6.945 IRQ/45 s; Ptyxis 8,44 %, GNOME 5,33 %, PowerTOP 3,62 %. Trotz hoher Last PC8 bereits 24,01 %, PC2 29,19 %, Package 2,62 W. PM_MAX/LTR und TB-off blieben korrekt | Wert nicht mit ruhigen Referenzen vergleichen. Wiederholung mit 30 s verzögertem Dienststart, anschließend 10 s Settle, damit UI sicher vorher geschlossen ist |

| 2026-08-06 23:23–23:24 | **Gültiger persistenter Kombinationslauf: `PM_MAX` + kompletter Thunderbolt-Stack off**, 45 s | Standalone/Akku, WLAN verbunden, internes Display an, Apps/Ptyxis geschlossen; Timer startete Dienst erst nach 30 s, dann 10 s Settle. Sehr ruhiges Fenster: Busy 0,53 %, `Any%C0` 3,94 %, GFX-RC6 99,98 %, i915 17 IRQ/45 s. WLAN-Traffic 15,6 KB RX/3,8 KB TX; SPA 23/23 bei 3.145.728 ns, SPB 23/23 bei 2.097.152 ns, jeweils `LTR_IGNORE=0` | **PC8 42,81 %**, PC2 43,37 %, PC7 3,07 %, PC6 0,37 %, PC9/10 0; Package 1,20 W, Core 0,25 W. Rohdeltas PC8 19.298.809 und PC2 19.550.489. Danach weiterhin `power_save_mode=1`, WLAN up, beide TB-Module absent, NHIs ungebunden, USB-C-xHCI an `xhci_hcd` | **Kombinationseffekt bestätigt:** Der persistente produktionsnahe Zustand erreicht die aus isolierten Tests erwarteten ca. 38–42 % PC8 und verdoppelt PC8 grob gegenüber `PM_MAX` mit Thunderbolt aktiv (20,36 %, allerdings höhere Last). Der absolute 1,20-W-Wert ist sehr gut, aber kein streng lastgematchter Energienachweis. PC10 bleibt durch weitere Plattformbedingungen blockiert |

| 2026-08-06, PM-Modus-Provenienz/BCM4377-Review | Historische und aktuelle Upstream-Quellen, Cypress-Downstream-Patch sowie gebautes DKMS-Modul read-only geprüft | `PM_MAX=1` wurde nicht aus `brcmfmac` entfernt: Bereits Linux v3.2 definierte `PM_OFF/PM_MAX/PM_FAST`, wählte im normalen cfg80211-Powersave-Pfad aber wie heute fest `PM_FAST`; aktuelles upstream nutzt `PM_MAX` weiterhin generisch für WoWLAN/Suspend. Der gefundene Patch wurde 2018 von Cypress-Ingenieur Chi-Hsien Lin signiert, blieb ein Downstream-Patch und bot `max_pm=1` optional bei weiterbestehendem PM_FAST-Default. Unser Patch entstand aus dem lokalen Upstream-Source-/LTR-Review, ist konzeptionell fast identisch, macht PM_MAX aber zum KaiT2en-Testdefault und validiert 1/2. | BCM4377 ist statisch kompatibel: gleicher `brcmfmac`-cfg80211-Befehlspfad, explizite 4377B3-Firmwarezuordnung, PCI-ID `14e4:4488`, wie BCM4364 `WCC_SEED`; das installierte Modul exportiert den 4488-Alias. Damit sollte es laden und `PM_MAX` senden. **Nicht ohne echte Hardware belegt** sind Annahme durch die konkrete Apple-4377-Firmware, resultierender LTR-Wert, PC8-Gewinn und Nebenwirkungsfreiheit. | Discord-Testaufruf für BCM4377 ist sinnvoll, aber als experimentell mit Stock-Restore und Pflichtdaten (Chip/Modell, Firmware, Modulpfad/Parameter, Verbindungs-/Suspendtests sowie vor/nach LTR/C-State) kennzeichnen. Ein Linux-Upstream-RFC ist nach Tests auf mehreren Chips sinnvoll; erfolgversprechender ist zunächst ein opt-in Modulparameter/Quirk mit PM_FAST als Default statt globalem PM_MAX-Default. |

| 2026-08-06, Branch-Rebase auf aktuellen Fork-Main | Nach Maintainer-Rebase `origin/main` von `0b0e6a7` auf `c13dfae` aktualisiert und `feat/brcmfmac-pm-max-dkms` konfliktfrei darauf rebased | `git range-diff` bestätigt alle vier Branch-Commits als inhaltlich identisch. Neue IDs: `7a84661` bounded-requests-`t2bdrm`, `8852f59` brcmfmac-DKMS, `01ee0c4` Standalone-Patch, `6931dca` Patch-Provenienz. Installer-Static-Check erfolgreich; isolierte Builds von brcmfmac und t2bdrm gegen `7.1.6-201.fc44` erfolgreich. brcmfmac-Metadaten weiterhin `PM_MAX`, Parameter 1/2 und BCM4377-Alias `14e4:4488`. | Die persönliche Thunderbolt-Deaktivierung ist gegen erneute normale KaiT2en-Installerläufe update-safe: Installer entfernt weder `/etc/modprobe.d/kait2en-disable-thunderbolt.conf` noch `/etc/dracut.conf.d/90-kait2en-disable-thunderbolt.conf` oder die Helfer unter `/usr/local/bin`; Dracut wertet `omit_drivers` beim Rebuild erneut aus. Kein erneuter Disable-Lauf nötig; nach Update optional Module/Bindung und `lsinitrd` verifizieren. | Rebaseten Remote-Branch per `git push --force-with-lease` aktualisieren. |

Branch-Stand nach Rebase: `7a84661` enthält den übernommenen bounded-requests-`t2bdrm`; `8852f59` enthält die geprüfte brcmfmac-PM_MAX-DKMS-Integration samt Restore und Dokumentation; `01ee0c4`/`6931dca` dokumentieren den Standalone-Patch. `local/learnings/power/c-states.md` und Messhelfer bleiben absichtlich untracked.

## Zwischenfazit und Hypothesen-Rangfolge

Zwei PC8-Demoter sind experimentell isoliert und reversibel repliziert. Erstens zieht brcmfmac im Firmwaremodus `PM_FAST` die Southport-A-LTR-Anforderung phasenweise auf 61,44 us: A→B hob PC8 durch ausschließlich diesen Ignore von 7,84 auf 21,45 %, revers B→A fiel es nach Restore von 20,18 auf 6,92 %. Der neue verbundene `PM_MAX`-Test bestätigt die produktionsnähere Ursache und Lösung ohne `LTR_IGNORE`: SPA lag 23/23 Samples bei 3,146 ms und PC8 stieg trotz höherer Last von 6,48 auf 20,36 %. Zweitens demotet der generische Thunderbolt-Treiber bzw. sein aktiver NHI/Host-Router-Zustand PC8, sein Effekt hängt aber stark vom WLAN-LTR ab: Bei SPA-Ignore ergab Generic off→on 38,15→21,46 %, bei normalem LTR brachte der komplette Stack-off dagegen nur 7,26→9,80 %. Die acht `t2thunderbolt`-PM-Links waren mit PC8 21,50→21,64 % neutral. Gleichzeitig erklärt der ruhige Display-on-Lauf, warum frühere interaktive PowerTOP-Messungen so schlecht aussahen: `Any%C0` lag jetzt nur bei 5–6 % statt 52–72 % und GFX RC6 bei praktisch 100 %. Der LTR-/PM_MAX-Effekt verschiebt vor allem PC6 nach PC8; ein lastgematchter Leistungs-A/B steht noch aus. PC10 blieb selbst mit beiden Blockern entfernt/ignoriert aus.

Priorität für die weitere Ursachenisolation:

1. **Bestätigter Root Cause und Lösungskandidat: brcmfmac `PM_FAST`/SPA-WLAN-LTR.** SOUTHPORT_A fordert damit phasenweise nur 61,44 µs statt 3,146 ms. WLAN-off und LTR-Ignore isolierten den Effekt; verbundenes `PM_MAX` bestätigte ihn ohne Vertragsbruch und hob PC8 von 6,48 auf 20,36 %. Netzwerkqualität und Leistung müssen vor einer Standardaktivierung noch validiert werden.
2. **Bestätigter großer Mess-/Nutzlastfaktor: aktive Desktop-Workload.** Der saubere Display-on-Lauf erreichte `Any%C0` 5,6 %, GFX RC6 99,99 % und bereits normal 7,84 % PC8. Interaktives PowerTOP, Ptyxis, Firefox, GNOME und Codex hatten frühere Fenster so stark belastet, dass der Plattformblocker maskiert wurde.
3. **Hoch, aber nicht alleiniger Root Cause: Intel-Displaypfad.** Display-off reduziert Package-Leistung von 4,68 auf 2,25 W und i915 von ~348 auf ~6 IRQ/s, erzeugt aber weiterhin praktisch kein PC8. DC6 wurde seit Boot nie erreicht, PSR ist nicht verfügbar und FBC ist wegen XR30 aus.
4. **Bestätigter, aber LTR-abhängiger PC8-Demoter: generischer `thunderbolt`-Treiber/NHI-Initialisierung.** Bei SPA-Ignore hob kombinierter Unload PC8 20,84→41,92 %; Link-only war mit 21,50→21,64 % neutral; reverser Generic-only-Test ergab off→on 38,15→21,46 %. Bei normalem WLAN-LTR brachte kompletter Stack-off dagegen nur 7,26→9,80 % PC8 und 1,21→1,13 W. Das KaiT2en-Linkmodul ist entlastet; TB-off allein erreicht das praktische PC8-Ziel nicht.
5. **Mittel: T2-spezifische periodische Arbeit.** BCE-Timestamp-Timer (6,67/s), Fan-Schleife (10/s) und Touch-Bar-UI/USB-Blits sind reale, vermeidbare Wakeups. Einzeln sind ihre gemessenen Raten klein; zusammen können sie kurze tiefe Residencies abschneiden.
6. **Niedrig nach aktueller Evidenz: PM-QoS.** Kein Prozess außer TuneD hält das Device offen, und dessen aktives Profil schreibt keinen Grenzwert.
7. **Weiter offen: Kernel-/Firmware-/Installationsdelta.** Der einmalige historische PC8-Wert bei eingeschaltetem Display und nur Ptyxis plus Firefox beweist, dass T2-Linkfähigkeiten und das Panel allein keine absolute Hardware-Sperre sind. Relevante Unterschiede könnten laufende Dienste, WLAN-/LTR-Zustand, Kernel, DSDT/SSDT oder Geräteinitialisierung sein.

Historischer Kontext: Der erste nachweisbare PowerTOP-Lauf war bereits auf Kernel 7.1.3, mit verbundener BCM4364 und laufenden `react-drm`-/Fan-Diensten. Er fand nach S3-Resume statt; auch der aktuelle Boot wurde jedoch schon vor den Messungen einmal aus S3 fortgesetzt. Diese Merkmale sind daher einzeln keine ausreichende Erklärung.

Warum PowerTOP `--auto-tune` plausibel wirkungslos blieb:

- `power/control=auto` erlaubt Runtime-Suspend nur; aktive Nutzer, DMA, IRQs oder fehlende Treiber-Callbacks können ein Gerät trotzdem in D0 halten.
- Das Firmware-FADT deklariert ASPM als nicht OS-steuerbar; bestehende Linkzustände sind teilweise aktiv, aber PowerTOP kann fehlende L1.2-Fähigkeit oder globale Firmwarepolitik nicht reparieren.
- Display-PSR/DC-State-Fähigkeiten, laufende GNOME-Compositor-Arbeit und feste T2-Timer sind keine einfachen PowerTOP-Tunables.
- Unter hoher CPU-/Display-Last fehlt schon das gemeinsame All-Core-Idle-Fenster, bevor Device-Autosuspend entscheidend wird.

Technische Referenzen (nicht Ice-Lake-spezifisch, daher nur zur Einordnung):

- Intel beschreibt Package-C2 explizit als möglichen Demotionszustand, wenn LTR oder nahe Timer tiefere Zustände verhindern, und nennt Core-idle, Graphics-RC6 sowie Device-LTR als Abhängigkeiten: <https://edc.intel.com/content/www/us/en/design/ipla/software-development-platforms/client/platforms/alder-lake-desktop/12th-generation-intel-core-processors-datasheet-volume-1-of-2/006/package-c-states/>
- Intel nennt Display an/aus, Auflösung, Anzahl Displays und PSR als Einflussgrößen; ohne PSR kann auf der dort beschriebenen Generation dennoch PC8 möglich sein: <https://edc.intel.com/content/www/us/en/design/ipla/software-development-platforms/client/platforms/alder-lake-desktop/12th-generation-intel-core-processors-datasheet-volume-1-of-2/001/package-c-states-and-display-resolutions/>
- Die i915-Dokumentation beschreibt, dass DMC/HW bei tieferen Display-C-States Display-Clocks und Power-Rails abschalten: <https://docs.kernel.org/6.13/gpu/i915.html>

## Nächste kontrollierte Tests (noch **nicht** ausgeführt)

1. Den erfolgreichen brcmfmac-`PM_MAX`-Lösungskandidaten auf Nebenwirkungen prüfen: zunächst Ping/Loss/Latenz und Durchsatz unter Stock/`PM_MAX`, danach Roaming/Reconnect sowie S3/WoWLAN. Zusätzlich einen lastgematchten längeren Leistungs-A/B ausführen; der aktuelle PC8-Beweis ist stark, die Wattwerte sind wegen deutlich unterschiedlichem Traffic nicht vergleichbar. Erwarteter Tradeoff ist höhere WLAN-Latenz.
2. Historischen tatsächlichen Generic-Driver-/Domain-Zustand weiter rekonstruieren. Die Warnung allein belegt keinen Probe-Abbruch; prüfen, ob ältere Fedora-Patches, Initramfs-Reihenfolge oder ein Post-S3-Zustand den NHI/Host-Router anders hinterließen.
3. Separaten Kernel-Entwicklungstest planen: prüfen, ob Runtime-PM für den integrierten Ice-Lake-TB3-Host-Router sicher aktivierbar ist, statt Thunderbolt global zu deaktivieren. Dafür Upstream-Hardwareannahmen und Suspend/Hotplug/Dock-Funktion gründlich testen.
4. Bei identischer Bedingung nur `react-drm.service` stoppen, messen, danach sofort wieder starten. Neben Package-C-States insbesondere BCE-DMA, USB5, Node-Wakeups und Package-Watt vergleichen.
5. Nur `t2-fancontrol.service` stoppen, messen, danach wieder starten. Erwarteter Effekt klein; Test dient der Quantifizierung.
6. Erst danach PowerTOP-`Bad`-Geräte einzeln statt pauschalem `--auto-tune` auf `auto` setzen und jeweils messen/zurücksetzen (PCH-XHCI, T2-Funktionen). So lassen sich Effekt und Instabilität einem Gerät zuordnen.

Messablauf ab 2026-08-06: C-State-Läufe als unabhängigen transienten systemd-Dienst starten und danach den Codex-Working-Mode sofort beenden; Ptyxis darf am normalen Prompt offen bleiben. Für erwartete große Effekte 45-s-Fenster plus Settle verwenden. Längere Fenster oder Replikate nur für kleine Leistungsunterschiede.

## Noch offen

- Lock-Test mit der erweiterten Skriptversion wiederholen, damit Prozess-CPU und PMC-LTR exakt innerhalb desselben Fensters erfasst werden.
- Ursache dafür, dass beide Ice-Lake-Thunderbolt-NHIs trotz leerer Ports und `power/control=auto` nie runtime-suspendiert waren.
- Ursprung der ~11 Intel-Atomic-Commits/s in einem ansonsten unbewegten Desktopfenster nach Ende der aktiven Diagnose erneut prüfen.
- Broadcom-LTR-Anforderung bei aktivem Link verstehen und prüfen, ob sie durch Treiber/Firmware, Traffic oder eine PCIe-LTR-Fehlprogrammierung entsteht; erst danach `ltr_ignore` erwägen.

## Upstream-Kandidat nach mehrtägigem Alltagstest (2026-08-09)

- Maintainer meldet nach mehreren Tagen mit BCM4364 und `PM_MAX` keine
  Auffälligkeiten sowie spürbar geringeren Akkuverbrauch.
- Der geplante Upstream-Patch dreht die bisherige KaiT2en-Testsemantik um:
  `PM_FAST` bleibt ohne Option der unveränderte Standard; der neue read-only
  Bool-Modulparameter `max_pm=1` wählt `PM_MAX` für normalen verbundenen
  Powersave. `PM_OFF`, P2P-Ausnahme und WoWLAN-Pfad bleiben unverändert.
- Implementierung folgt der brcmfmac-Modulparameterarchitektur und dem
  Cypress-Downstream-Vorläufer von Chi-Hsien Lin (2018): `default_pm` wird pro
  Gerät in `brcmf_mp_device` gespeichert und an den beiden bisherigen
  `PM_FAST`-Aufrufstellen verwendet. Upstream-Diff: 3 Dateien, +10/-2.
- Standalone-Patch basiert auf aktuellem `wireless-next` HEAD
  `ca800a9302764c445de0da0e84d2252400a770ee` und liegt unter
  `patches/brcmfmac/0001-wifi-brcmfmac-allow-opting-into-PM_MAX.patch`.
  Er lässt sich dort sauber anwenden. `checkpatch --strict` ist bis auf den
  absichtlich noch fehlenden menschlichen `Signed-off-by` sauber.
- KaiT2en enthält exakt dieselbe Treiberänderung. Der Installer aktiviert sie
  mit `/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf`; Restore entfernt die
  Option vor dem Stock-Reload. DKMS-Version ist `7.1.6_kait2en2`; die
  Integration ist als `030422e` auf `feat/brcmfmac-pm-max-dkms` committed,
  aber noch nicht gepusht oder auf dem laufenden System installiert.
- Isolierter Build gegen Fedora `7.1.6-201.fc44` erfolgreich. `modinfo` zeigt
  `max_pm` als bool; das Symbol liegt ohne Option in BSS und ist damit false,
  während KaiT2ens Modprobe-Datei `max_pm=1` setzt. Installer-Static-Checks
  erfolgreich.
- Autor-Identität ist auf Maintainer-Wunsch `Alexander Fischer
  <alexander@fischermail.me>`. Der DCO-Sign-off bleibt bis zur persönlichen
  Prüfung durch Alexander absichtlich aus. `Assisted-by` wurde auf seinen
  ausdrücklichen Wunsch vollständig entfernt; dies weicht von der aktuellen
  Kernel-Dokumentation zu Coding Assistants ab und kann im Review beanstandet
  werden.
- Aktuelle Empfänger aus wireless-next `MAINTAINERS`: To
  `linux-wireless@vger.kernel.org`; Cc Arend van Spriel,
  `brcm80211@lists.linux.dev` und `brcm80211-dev-list.pdl@broadcom.com`.
  Lokales `git-email` und `perl-open` sind noch nicht installiert.

### Veröffentlichung und Installation des Opt-in-Stands (2026-08-09)

- Commit `030422e` erfolgreich auf
  `origin/feat/brcmfmac-pm-max-dkms` gepusht; lokale und Remote-ID stimmen
  exakt überein.
- Ausschließlich `scripts/fedora/install-dkms-modules.sh` plus
  `rebuild-initramfs.sh` ausgeführt. Alle DKMS-Module bauten und installierten
  erfolgreich; brcmfmac ist jetzt `7.1.6_kait2en2` unter
  `/lib/modules/7.1.6-201.fc44.x86_64/extra/brcmfmac.ko.xz`.
- Installierter brcmfmac-Source stimmt per SHA-256 exakt mit dem Branch überein;
  `modinfo -p` zeigt den booleschen `max_pm`-Parameter. Installierte und im
  Initramfs enthaltene Modprobe-Datei setzt `options brcmfmac max_pm=1`.
- Der laufende brcmfmac ist erwartungsgemäß noch die vorherige Testversion und
  zeigt `power_save_mode=1`; das neue `/sys/module/brcmfmac/parameters/max_pm`
  kann erst nach Reboot erscheinen. Kein Live-Reload, weil die einzige
  Default-Route über WLAN läuft und dadurch die Codex-Session abbrechen würde.
- Bounded-requests-`t2bdrm` bleibt installiert: DKMS-Source enthält
  `rows_per_request`/die geteilten Requestpfade und das installierte Modul die
  eindeutige Zeichenkette `Using %u rows per framebuffer request`.
- Persönliche Thunderbolt-Deaktivierung blieb erhalten: beide Modprobe-Ziele
  lösen zu `install /bin/false` auf, beide Module sind ungeladen, Dracut
  `omit_drivers` ist vorhanden und das neue Initramfs enthält keine TB-Module.
- Nächster zwingender Schritt: manueller Reboot. Danach `max_pm=Y`, Modulpfad,
  SOUTHPORT_A-LTR 3.145.728 ns, WLAN-Assoziation, Thunderbolt-off und t2bdrm
  erneut verifizieren.

### Reboot-Verifikation des Upstream-identischen Opt-in-Patches (2026-08-09)

- Neuer Boot `b42817bf-5387-40b5-809b-674a1087a5ae`, Kernel weiterhin
  `7.1.6-201.fc44`. Geladen/aufgelöst wird
  `/lib/modules/.../extra/brcmfmac.ko.xz`; DKMS bestätigt
  `brcmfmac_kait2en/7.1.6_kait2en2` installiert.
- Neuer read-only Sysfs-Parameter ist vorhanden und eindeutig aktiv:
  `/sys/module/brcmfmac/parameters/max_pm=Y`. Der alte KaiT2en-spezifische
  `power_save_mode`-Parameter ist erwartungsgemäß verschwunden.
- BCM4364/4 (`14e4:4464`, rev 04) ist an brcmfmac gebunden und mit
  `Fischer-Netz` auf 5 GHz/5220 MHz verbunden; Signal -44 dBm, RX/TX 400
  MBit/s, cfg80211 Power Save an. Firmware weiterhin
  `9.30.514.0.32.5.94`, FWID `01-68d7ff80`.
- Hardwarewirkung bestätigt: SOUTHPORT_A Non-Snoop und Snoop jeweils exakt
  **3.145.728 ns**, `LTR_IGNORE=0`; SOUTHPORT_B 2.097.152 ns. Damit sendet der
  upstream-identische Opt-in-Patch tatsächlich PM_MAX und erreicht denselben
  gewünschten LTR-Zustand wie die vorige Testimplementierung.
- Bounded-requests-`t2bdrm` läuft aus `/extra`, USB-Interface ist gebunden,
  Modul enthält die eindeutige `rows per framebuffer request`-Zeichenkette;
  Bootlog zeigt normale DRM-Initialisierung und keine relevanten Warnungen.
- Thunderbolt-off überstand Reboot: beide Module ungeladen und per
  `install /bin/false` blockiert; NHIs `00:0d.2/.3` ungebunden. USB-C-xHCI
  `00:0d.0` bleibt an `xhci_hcd`. Keine relevanten brcmfmac-, t2bdrm-, DMA-,
  Allocation- oder Thunderbolt-Warnungen im aktuellen Boot.
- **Ergebnis:** Exakt der zur Einreichung vorgesehene Treiberdiff läuft lokal
  mit KaiT2en-Opt-in fehlerfrei und reproduziert den gewünschten PM_MAX/LTR-
  Zustand. Community-Alltagstest kann auf Commit `030422e` beginnen.

### PM_MAX-Historie und Co-Maintainer-Review (2026-08-10)

- Upstream-Blame gegen aktuelles Linux/wireless-next: Beide normalen
  `PM_FAST`-Stellen sowie `PM_OFF`, `PM_MAX` und `PM_FAST` waren bereits beim
  ersten Mainline-Import von brcm80211, Commit
  `5b435de0d786869c95d1962121af0d7df2542009` von 2011, vorhanden. Die ältere
  Staging-Historie führt die verbundene `PM_FAST`-Auswahl weiter bis zum
  ursprünglichen FullMAC-Import `cf2b448852abd47cee21007b8313fbf962bf3c9a`
  von 2010 zurück. Keiner der Import-Commits dokumentiert eine
  Auswahlbegründung; ein früherer Wechsel von `PM_MAX` zu `PM_FAST` wurde
  nicht gefunden.
- Die heutige `PM_MAX`-Verwendung für WoWLAN kam mit Commit
  `4eb3af7c1df32dcd4362c2f20928f679ed78f2e5` von 2014 hinzu. `PM_MAX` ist
  damit ein seit Langem vorhandener Firmwaremodus, aber kein bisheriger
  normaler Connected-Default.
- Cypress führte 2018 downstream genau einen opt-in-Parameter `max_pm` ein.
  Purism übernahm diesen Patch 2023 und notierte als möglichen Nachteil eine
  deutlich höhere Latenz. Das ist der plausible technische Grund, den
  Mainline-Default nicht blind zu ändern: `PM_FAST` priorisiert Reaktionszeit,
  `PM_MAX` längere Schlafphasen. Ein dokumentierter Mainline-Entscheid gegen
  `PM_MAX` oder eine bekannte DMI-/Chip-Ausschlussliste wurde nicht gefunden.
- Co-Maintainer Andre Eikmeyer `<dev@deq.rocks>` hat den Patch geprüft und
  `PM_MAX` auf BCM4377 ohne beobachtete Regression getestet. Er hat
  `Reviewed-by` und `Tested-by` ausdrücklich freigegeben.
- Upstream-Commit-Message lokal überarbeitet: persönliche Motivation statt
  MacBook-Fokus, BCM4364- und BCM4377-Testabdeckung, möglicher
  Latenz-Trade-off und unveränderter `PM_FAST`-Default werden explizit
  erklärt. Eine menschliche Anrede gehört später in die Versandmail, nicht in
  die permanente Git-Commit-Message. Noch nicht pushen; Alexander möchte den
  Text selbst weiter vermenschlichen.

### Runtime-A/B-Helfer für PM_MAX und PM_FAST (2026-08-11)

- Der aktuelle `max_pm`-Parameter ist nur beim Laden des Moduls wirksam und
  mit Modus `0400` nicht beschreibbar. `iw ... set power_save off/on` wechselt
  nur zwischen `PM_OFF` und dem beim Modulstart gewählten Default, nicht
  zwischen `PM_FAST` und `PM_MAX`.
- Untracked Helfer `switch-brcmfmac-pm.sh` angelegt und per `bash -n` sowie
  ShellCheck geprüft. Modi: `max`, `fast`, `status`.
- Das Skript schaltet WLAN über NetworkManager kurz aus, entlädt
  `brcmfmac_wcc` vor `brcmfmac`, lädt `brcmfmac` explizit mit `max_pm=0/1`,
  stellt den vorher geladenen WCC-Zustand wieder her, aktiviert WLAN erneut
  und verifiziert den Sysfs-Wert. Bei Ladefehlern versucht es, den vorherigen
  Zustand wiederherzustellen.
- Der Wechsel ist absichtlich nur temporär. Die vorhandene
  `/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf` bleibt unverändert; nach einem
  Reboot gilt deshalb wieder `max_pm=1`/PM_MAX.
- Noch **nicht ausgeführt**, da jeder Wechsel die WLAN-Verbindung und damit
  gegebenenfalls die Codex-Sitzung unterbricht. A/B-Test erst in einem vorher
  angekündigten Testfenster, idealerweise mit aktivem kabelgebundenem Netz.

### PM_MAX-Alltagstest widerlegt den Lösungskandidaten (2026-08-14)

- Alexander reproduziert mit aktiviertem `PM_MAX` wiederholt zusätzliche
  WLAN-Latenzen von etwa **50–250 ms**. Nach dem Wechsel zurück auf `PM_FAST`
  treten sie nicht in derselben Form auf. Die im längeren Alltag beobachtete
  Batterieersparnis ist außerdem nur minimal.
- Damit ist der zentrale Satz der eingereichten Commit-Message („without
  observed connectivity or latency regressions“) durch bessere Langzeitdaten
  widerlegt. Die höhere PC8-Residency bleibt technisch real, rechtfertigt den
  erheblichen Latenz-Trade-off in der Praxis aber nicht.
- Patchwork-Patch `14741390`, Message-ID
  `<20260810150552.25375-1-alexander@fischermail.me>`, ist am 2026-08-14 noch
  `New`, nicht akzeptiert und Arend van Spriel zugewiesen. Sauberer Rückzug:
  kurze Plain-Text-Antwort im bestehenden Thread an alle bisherigen Empfänger,
  reproduzierbare Regression und geringen Nutzen nennen, um Drop bitten und
  `pw-bot: changes-requested` als Metadatum setzen. Keine v2 geplant.
- Der Wifibot-Check `deprecated_api` schlägt zusätzlich korrekt an, weil der
  Patch die Zahl neuer `module_param`-Verwendungen von 0 auf 1 erhöht. Das ist
  ein unabhängiger API-/Review-Einwand; primärer Rückzugsgrund ist die real
  gemessene Nutzerregression.
- Lokales System ist noch **nicht persistent restauriert**: Für Kernel 7.1.6
  und 7.1.7 ist `brcmfmac_kait2en/7.1.6_kait2en2` installiert,
  `modinfo` löst nach `/extra/brcmfmac.ko.xz` auf und
  `/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf` setzt weiterhin `max_pm=1`.
  Ein per Umschaltskript gesetztes `PM_FAST` gilt nur bis Modul-Reload/Reboot.
  Das vorhandene gezielte Restore-Skript entfernt ausschließlich den
  brcmfmac-DKMS-Stand und die PM_MAX-Option; `t2bdrm` bleibt davon unberührt.

### Vollständiger PM_MAX-Rückzug und lokaler On-Disk-Restore (2026-08-15)

- Rücknahmemail ist im originalen linux-wireless-Thread angekommen,
  Message-ID `<20260814205115.799977-1-alexander@fischermail.me>`. Patchwork
  führt den Patch noch als `New`, enthält die Antwort aber bereits als
  Kommentar `27135273`; der Maintainer-/Bot-Status kann verzögert folgen.
- `brcmfmac_kait2en/7.1.6_kait2en2` für Kernel 7.1.6 und 7.1.7 vollständig aus
  DKMS entfernt. `/usr/src` und `/var/lib/dkms` enthalten keine Paketreste,
  `/extra/brcmfmac.ko*` ist für keinen Kernel mehr vorhanden. Beide Kernel
  lösen wieder zum Fedora-Modul unter `/kernel/drivers/net/wireless/...` auf.
- `/etc/modprobe.d/kait2en-brcmfmac-pm-max.conf` und der
  `brcmfmac_kait2en`-Eintrag im Kernel-Hook sind entfernt. Beide vorhandenen
  Initramfs enthalten keinen KaiT2en-brcmfmac. Nächster Modulstart lädt Stock
  mit ausschließlich `p2pon=0`.
- Kein Live-Reload, weil WLAN die einzige Default-Route ist. Das noch im RAM
  befindliche Testmodul meldet `max_pm=N`; nach Reboot verschwindet der
  Parameter vollständig und Fedora-Stock ist auch runtime aktiv.
- `t2bdrm/0.1` blieb für beide Kernel installiert. Pfad weiterhin `/extra`,
  aktueller Modulhash unverändert
  `86c102e30e4cf35eb645bfe118ebc39d2d26fa1f3e208ba0d4ab0c90b09505ab`.
  Persönliche Thunderbolt-Modprobe-/Dracut-Deaktivierung blieb ebenfalls
  unverändert.
- Restore-Skriptfehler entdeckt und korrigiert: `dkms status` listet dieselbe
  Paketversion einmal pro Kernel; die Schleife versuchte deshalb nach
  erfolgreichem `dkms remove --all` denselben Versions-Remove erneut. Die
  Versionen werden nun mit `sort -u` dedupliziert; idempotenter Folgelauf und
  vollständige Verifikation erfolgreich.
- Öffentlicher Branch `feat/brcmfmac-pm-max-dkms` per Fast-Forward von
  `030422e` auf Retirement-Commit `1ac6baa` aktualisiert. README warnt deutlich,
  Installer und Kernel-Hook installieren brcmfmac/PM_MAX nicht mehr, Restore
  bleibt für frühere Tester erhalten. Der absichtlich lokale Commit `8c63ad9`
  wurde nicht veröffentlicht. Installer-Static-Checks bestanden.
- Separater, noch nicht als PR eingereichter Statusbranch
  `docs/revoke-brcmfmac-pm-max` bleibt auf Fork-Commit `d184290`; dort steht
  `upstream: revoked` mit der BCM4364-Latenzbegründung.

## Re-measurement on kernel 7.1.8 with Thunderbolt off and stock brcmfmac (2026-08-22)

Conditions for every window below: MacBookPro16,2, kernel `7.1.8-200.fc44`,
battery only, internal display on at 7400/17777 backlight, WLAN associated
(`Fischer-Netz`, 5 GHz), Thunderbolt still blocked by the personal modprobe/
dracut rule, Fedora-stock `brcmfmac` (PM_FAST, `max_pm` gone), idle desktop
with only Ptyxis plus the Claude CLI. Every A/B change was reverted by an EXIT
trap and the restored state was verified afterwards. Raw data lives under the
session scratchpad `results/`, `results2/` … `results5/`.

### New primary metric: battery gauge, not package residency

`turbostat` only reports ~1.2 W of package power while `BAT0`
(`current_now × voltage_now`) shows **7.2–7.4 W** total system draw. Package
C-state percentages therefore describe at most a sixth of the budget, and the
sign of a residency change does not predict the sign of the power change.
All conclusions below are based on the battery gauge.

### Idle baseline

- Battery draw 7.20–7.36 W; package (RAPL) 1.14–1.20 W, cores 0.28–0.30 W.
- `Any%C0` 5.4–6.1 %, `GFX%rc6` 99.99 %.
- Package residency: PC2 ≈ 41 %, PC6 ≈ 49 %, PC7/PC8 ≈ 0, PC9/PC10 = 0.
- Numbered IRQs per 60 s: NVMe ~1050, `bce_dma` ~490, ACPI/GPE ~340,
  `brcmf_pcie_intr` ~300–430, i915 6–22. Everything else is timer ticks.

### The central negative result: PC6 → PC8 buys nothing here

Three interleaved 90 s pairs (`E1a/E1b`, `E2a/E2b`, `E3a/E3b`) with the
backlight pinned by re-writing it every 2 s and `SOUTHPORT_A` sampled every 2 s:

| pair | PC6 → PC8 | battery W base → SPA `ltr_ignore` |
|---|---|---|
| E1 | 48.54 % → 44.96 % PC8 | 7.330 → 7.347 |
| E2 | 48.16 % → 44.70 % PC8 | 7.326 → 7.348 |
| E3 (WLAN went quiet by itself, confounded) | 32.29 % → 44.96 % PC8 | 6.956 → 6.726 |

In the two clean pairs, converting ~45 percentage points of residency from PC6
to PC8 changed system power by **+0.02 W**, i.e. nothing, while package RAPL
even rose from 1.19 to 1.30 W. **This retroactively explains the PM_MAX result
of 2026-08-14** ("higher PC8, only minimal battery saving"): on this platform
PC6 and PC8 are energetically almost equivalent, so the latency trade-off of
PM_MAX could never have paid off. Package-C8 residency is the wrong target
metric for battery life on this machine.

### What actually costs power

| change | battery W | delta |
|---|---|---|
| backlight 1777/17777 (10 %) | 5.455 | −1.8 |
| baseline 7400/17777 (41.6 %) | 7.20–7.36 | — |
| backlight 17777/17777 (100 %) | 9.875 | +2.6 |
| WLAN radio off (`nmcli radio wifi off`) | 6.368 vs 7.296 | **−0.93** |
| "quiet userspace" (docker, containerd, cups, avahi, ModemManager, smartd, abrt*, passim, geoclue, dnf5daemon, pcscd, rsyslog, sysstat/snapper timers, fan daemon all stopped) | 7.298 vs 7.319 | −0.02 (nothing) |
| `react-drm` stopped (Touch Bar UI) | 7.287 vs 7.204/7.281 | none |
| PCH xHCI `0000:00:14.0` `power/control` on → auto | 0.98 vs 1.01 pkg W | none |
| `t2-fancontrol` stopped | — | none |

The backlight spans ~4.4 W between 10 % and 100 % and dominates everything
else combined. WLAN is second at ~0.4–0.9 W depending on how chatty the link
currently is; after a reconnect the same idle desktop dropped from ~7.33 W to
~6.85 W and reached ~21 % PC8 without any intervention, purely because
`SOUTHPORT_A` then reported 3.146 ms instead of 61.44 µs.

### Platform blockers confirmed read-only

- **Firmware denies OS control of ASPM.** `_OSC` grants only
  `[PCIeHotplug SHPCHotplug AER PCIeCapability LTR DPC]`. `pcie_aspm=force`
  gets L0s/L1 back, but **every** PCIe link reports
  `L1SubCtl1: PCI-PM_L1.2- PCI-PM_L1.1- ASPM_L1.2- ASPM_L1.1-`. Writing `1` to
  `/sys/bus/pci/devices/*/link/l1_{1,2}_{aspm,pcipm}` returns success and reads
  back `0` on every device tried (WLAN endpoint, its root port, T2/NVMe, TB
  root port, MEI, HDA). So there are no L1 substates anywhere. This contradicts
  the 2026-08-05 note that the WLAN path had L1.2 active; the current state is
  unambiguous.
- **No PSR, therefore no DC6 and no PC10.** `i915_edp_psr_status` returns
  `ENODEV` although `has_psr: yes` in `intel_display_caps`. `DC5 -> DC6 count`
  is 0 since boot and stayed 0 in every window. PC9/PC10 were 0 in all 25
  measured windows. (The reason is NOT a panel limitation - see the
  2026-08-23 section below, which corrects this entry.)
- **FBC off:** `i915_fbc_status` says `pixel format not supported`; the primary
  plane runs XR30 (10 bpc).
- **`intel_idle` runs in ACPI `_CST` fallback**, states `C1_ACPI` / `C2_ACPI` /
  `C3_ACPI` with MWAIT hints `0x00` / `0x31` / `0x60`. The deepest hint is C10,
  so the core side is not the limit; average `C3_ACPI` residency is ~28 ms per
  entry. Only the `menu` governor is registered — `teo` is not available.
- **Apple ANS2 NVMe exposes exactly one power state** (`ps 0 : mp:0.00W`, no
  APST, `get-feature 0x0c` rejected) and has no runtime PM, so it is
  permanently D0. `nvme_core.default_ps_max_latency_us` is irrelevant here.
- **PC2 sits at ~41 %** under every condition tested with the display on,
  including WLAN off, LTR ignore, quiet userspace and Touch Bar off. The
  display-off test below identifies the display engine as its cause.

### Wakeup attribution (PowerTOP, 60 s idle)

`tick_nohz_handler` 147.7/s, claude 19.8/s, **wireplumber 19.3/s**,
`psi_avgs_work` 15.9/s, sched softirq 12.9/s, block softirq 11.9/s,
`btrfs_work_helper` 11.1/s, `simple_end_io_work` 9.3/s, **onedriver 8.8/s**,
`usb_giveback_urb_bh` 7.9/s, `t2-fancontrol` 6.9/s, `irq/149-bce_dma` 6.6/s,
`irq/159-brcmf` 4.6/s, `irq/9-acpi` 4.1/s, containerd 3.3+1.3+0.6/s,
`bce_xhci_pm_tick` 510 µs/s CPU (the 150 ms `t2bce_core` timer, 6.67/s).

None of these is worth chasing for battery life: the "quiet userspace" window
removed a large share of them and saved 0.02 W. `wireplumber` at 19.3/s with
every PipeWire node reported `suspended` (only `effect_output.t2-162-speakers`
is `idle`) is the one entry that looks like a genuine bug rather than
necessary work, but it too is below the measurement noise floor.

### Consequence for future work

Stop optimising package C-state residency on this machine. The remaining
levers in descending order of measured value are backlight, WLAN radio
activity, and then nothing above ~50 mW. Any future claim of a power win must
be backed by a paired battery-gauge measurement with the backlight pinned,
because backlight auto-dim alone moved a baseline by 0.5 W mid-run and
invalidated an entire measurement round.

### PC2 root cause: the display engine (2026-08-22, follow-up)

Run as three 90 s windows via `org.gnome.ScreenSaver.SetActive`, with
`lock-enabled` and `sleep-inactive-battery-type` temporarily neutralised and
restored by trap (verified back to `true` / `'suspend'` afterwards).

| window | PC2 | PC6 | PC8 | `Any%C0` | IRQ/90 s | battery W |
|---|---|---|---|---|---|---|
| H0 display on | 38.97 % | 44.85 % | 0 | 9.88 % | 63 798 | 7.588 |
| H1 **display off** | **10.47 %** | 57.15 % | 0 | 18.69 % | 166 624 | 4.640 |
| H2 display on | 40.68 % | 47.41 % | 0.54 % | 5.96 % | 50 463 | 7.259 |

**The display engine owns ~30 percentage points of PC2.** The blank window had
roughly three times the CPU load and IRQ rate of both display-on windows
(GNOME's blank/unblank machinery ran inside it), so the effect is understated,
not inflated. With no PSR and no FBC the engine fetches the 2560x1600 XR30
framebuffer from RAM continuously, which keeps the memory controller out of
deeper package states; nothing else moved PC2 in 28 measured windows.

The two gates are independent and stack in sequence:

- **display engine** gates PC2 → PC6
- **WLAN LTR** gates PC6 → PC8

H1 proves this: with the display off, PC2 collapsed but PC8 stayed at 0,
because `SOUTHPORT_A` was still reporting 61.44 µs for the whole window.

`DC5 -> DC6` stayed 0 even with the screen blanked and `DC3 -> DC5` incremented
only once, so GNOME blanking does not reach a real DPMS-off/DC9 state here.
The 4.64 W figure is not a clean display-engine cost, because the backlight was
deliberately not pinned in that window; against the measured 5.455 W at 10 %
backlight it suggests the panel electronics plus engine are worth a few hundred
mW beyond the backlight itself. PC10 remained 0 even here.

### What the WLAN LTR value actually is (2026-08-22)

Register decode (bit 15 = requirement valid, bits 12:10 = scale, bits 9:0 =
value):

- `0x883c` → scale 2 (x1024 ns), value 60 → **61 440 ns**
- `0x9003` → scale 4 (x1 048 576 ns), value 3 → **3 145 728 ns**

The endpoint's own config-space LTR registers read 0; Linux never programs
this value anywhere. The BCM4364 firmware emits LTR messages live over PCIe and
the PCH aggregates them into `SOUTHPORT_A`. The number is therefore a **live
readout of the WLAN firmware's internal power state**, not a setting:

- 61.44 µs = firmware is in its shallow awake state (PM_FAST awake window,
  post-traffic linger, beacon reception)
- 3.146 ms = firmware is in deep sleep — the identical value seen with the
  radio switched off and with PM_MAX

This is why the value relaxed by itself after a reconnect: the background
sockets had not yet resumed polling, so the firmware genuinely stayed asleep.

**Consequence: the LTR is a thermometer, not a lever.** `ltr_ignore` makes the
platform ignore the thermometer, which changes package-state bookkeeping and
saves 0.00 W, because the radio keeps drawing the same current. That
decomposes the measured 0.93 W of "WLAN off" into ~0.00 W of LTR/C-state effect
and ~0.93 W of actual radio (PHY, RX chain, beacon reception, TX). There is
nothing to win from the LTR itself; the only WLAN lever is making the radio
sleep more deeply and more often.

### Middle ground between PM_FAST and PM_MAX

`BRCMF_C_SET_PM` accepts only 0/1/2 (OFF/MAX/FAST) — there is no intermediate
mode, which is exactly why PM_MAX was so blunt. The firmware does however
expose separate iovars that shape PM_FAST's behaviour:

- **`pm2_sleep_ret`** — how long the firmware lingers awake after the last
  packet before dropping back to sleep in PM2/PM_FAST (typically 200 ms).
  Shortening it only trims the post-traffic tail and does not change the
  response time to incoming packets. This is the genuine middle ground.
- **`bcn_li_dtim`** — listen interval in DTIM periods; lets the firmware skip
  beacons while idle. Saves radio wakeups at the cost of downlink and broadcast
  latency.

These are reachable from userspace **without any driver change**: the loaded
module exports `brcmf_cfg80211_vndr_cmds_dcmd_handler` (verified in
`/proc/kallsyms`), the nl80211 vendor DCMD passthrough that Broadcom's `wl`
utility uses. Untested so far. Realistic expectation: the entire radio is
0.93 W, so tail tuning can plausibly reach 100–300 mW at true idle, and only
there.

The likely larger WLAN lever remains untested: the "quiet userspace" window did
**not** stop the network-chatty user processes (onedriver at 8.8 wakeups/s,
Nextcloud, forticlient), and `SOUTHPORT_A` stayed pinned at 61.44 µs for that
entire window — the chatter was still present. A targeted A/B against those
three is the obvious next experiment.

### FBC remains the only untested display lever

FBC is off solely because the primary plane runs XR30 (10 bpc);
`org.gnome.mutter experimental-features` is empty, so the 10-bit choice is
mutter's own default for this panel and not a flag that can simply be cleared.
Compressing the framebuffer would cut the engine's memory fetch and is the only
plausible way to move PC2 while the display stays on. Whether PC2 → PC6 is
worth any power at all is unknown: no paired measurement ever varied PC2 with
the display on. Note that PC6 → PC8 was measured at 0.02 W, so the assumption
that deeper is better must not be carried over untested.

### PSR is blocked by a DRM quirk, not by the panel (2026-08-23)

This **corrects** the earlier entries that claimed the panel cannot do PSR.
It can. Linux refuses to use it.

Raw DPCD read straight from the panel over `/dev/drm_dp_aux0`:

| DPCD | value | meaning |
|---|---|---|
| `0x000` DPCD_REV | `0x11` | DP 1.1 (this is the *DP* rev, it says nothing about PSR) |
| `0x700` eDP_DPCD_REV | `0x02` | **eDP 1.3** |
| `0x070` PSR_SUPPORT | `0x01` | **`DP_PSR_IS_SUPPORTED` - the panel advertises PSR1** |
| `0x071` PSR_CAPS | `0x01` | `DP_PSR_NO_TRAIN_ON_EXIT` - no link retraining on exit (cheap exits) |
| `0x701` EDP_GENERAL_CAP_1 | `0x83` | bit 7 `DP_EDP_SET_POWER_CAP` set |
| `0x400` SINK_OUI | `00:10:fa` | **Apple Inc.** |
| `0x403` device id | `65 44 13 67 64 61` | |

Yet `eDP-1/i915_psr_status` says `Sink support: PSR = no`.

`_psr_init_dpcd()` in `intel_psr.c` has exactly three gates after
`psr_dpcd[0] != 0`: the `DP_DPCD_QUIRK_NO_PSR` quirk, the `DP_EDP_SET_POWER_CAP`
bit, and then `sink_support = true`. `0x701` bit 7 is set, so only the quirk can
explain it. Confirmed by decoding the `dpcd_quirk_list` table out of the
`.rodata` of `drm_display_helper.ko` (struct is `u8 oui[3]; u8 device_id[6];
bool is_branch; u32 quirks;`, 16 bytes per entry, table at file offset
`0x45540`):

```
0x45540  00 22 b9 000000000000 01 0000 01000000   Analogix,   ANY,      CONSTANT_N
0x45550  00 22 b9 736976617254 00 0000 01000000   "sivarT",             CONSTANT_N
0x45560  00 10 fa 000000000000 00 0000 02000000   Apple,      ANY,      NO_PSR   <=
0x45570  00 00 00 434837353131 00 0000 04000000   "CH7511",             NO_SINK_COUNT
```

`quirks = 0x02 = BIT(1) = DP_DPCD_QUIRK_NO_PSR`, matched on the Apple OUI with
`DEVICE_ID_ANY`. **Every Apple eDP panel is blanket-banned from PSR in the DRM
core**, regardless of what it advertises. The upstream comment on that entry is
that Apple panels need additional handling to support PSR - handling that was
never implemented in Linux, so the quirk is a permanent ban rather than a
workaround. Our panel is not individually blacklisted; it is caught by the
wildcard.

Note the same table has a *second* Apple entry at `0x455b0` with device id
`65 44 15 65 62 61` and `quirks = 0x10`
(`CAN_DO_MAX_LINK_RATE_3_24_GBPS`). Our device id `65 44 13 67 64 61` does not
match that one, so only the NO_PSR wildcard applies here.

Consequences:

- Without PSR the display engine scans out 2560x1600 XR30 from memory
  continuously, which is what pins PC2 at ~40 % (see the display-off test).
  PSR would let the panel TCON self-refresh from its own frame buffer so the
  engine, the PLLs and the memory path can idle *with the image still on
  screen*. That is the only known mechanism on this platform that attacks PC2
  without turning the display off.
- No PSR also means no DC5/DC6 while the display is on, which is why
  `DC5 -> DC6 count` has been 0 since boot.
- PSR1 (not PSR2) means DC3CO does not apply - DC3CO is tied to PSR2 selective
  update. Only DC5/DC6 are in play.

There is **no runtime lever** for this: `i915.enable_psr` only controls the
source side, `CAN_PSR()` still requires `psr.sink_support`, and the quirk is
evaluated once at connector init. Changing it requires rebuilding
`drm_display_helper` (or the kernel) with that one table entry removed or
narrowed to specific device ids.

The experiment is safely reversible: even with the quirk gone,
`i915.enable_psr=0` on the kernel command line forces PSR off again, so a
broken boot is recoverable from the boot menu.

Expected payoff is **unquantified**. The display-off window measured
7.588 -> 4.640 W, but that window also had the backlight off, and backlight
alone is worth ~1.8-2.2 W here, so the scanout-side share is roughly
0.7-1.0 W - an order of magnitude more than the 0.02 W that PC6 -> PC8 was
worth, but still an estimate, not a measurement. PSR does not touch the
backlight.

Risk: the quirk exists because standard PSR misbehaved on Apple panels
(flicker, black screen, corruption on static content). This panel reports
`DP_PSR_NO_TRAIN_ON_EXIT`, which is the benign case, but that is not a
guarantee.

An executable plan for lifting that quirk and measuring the result lives in
`local/learnings/kernel/psr-quirk-plan.md` (untracked, repo root). Not started as of 2026-08-23.

## PSR quirk lifted and measured: works on the source, dead on the panel (2026-08-23)

`local/learnings/kernel/psr-quirk-plan.md` was executed end to end on `7.1.9-200.fc44.x86_64`.
Outcome: **negative, and the quirk is justified for this panel.** The stock
`drm_display_helper` has been restored. Do not repeat this without new
information - the six configurations below are already ruled out.

### The patch worked, but it is only half the gate

Setting the Apple wildcard entry in `dpcd_quirk_list[]` to a quirk mask of `0`
flipped `Sink support: PSR` from `no` to `yes [0x01]`, exactly as predicted.
Verified byte-for-byte in the built module: the only change in `.rodata` is
`02 -> 00` in that one table row; `.data`, `.bss` and the export list are
identical to the shipped module.

That alone changes nothing, because there is a **second Apple-specific gate**
the plan did not know about. In 7.1.9 `psr_global_enabled()` reads, for eDP,
*only* the VBT:

```c
case I915_PSR_DEBUG_DEFAULT:
	return intel_dp_is_edp(intel_dp) ?
		connector->panel.vbt.psr.enable : true;
```

Apple's VBT does not enable PSR, so PSR stays off even with the quirk gone.
Note that `i915.enable_psr=1` does **not** help here - the module parameter is
not consulted on this path at all. The only lever is
`echo 2 > .../i915_edp_psr_debug` (`I915_PSR_DEBUG_ENABLE`), which forces
`psr_global_enabled()` true and triggers the modeset. A permanent setup would
therefore need the patched module *and* a boot-time debugfs write.

### The power saving is real and larger than estimated

Interleaved A/B, 3 rounds, 90 s windows, backlight pinned, on battery,
GNOME blanking and auto-dim temporarily disabled (they would have fired at
600 s and ruined the run):

| | battery gauge | Pkg%pc6 | DC5 -> DC6 |
|---|---|---|---|
| PSR forced on | **7.144 W** | 64-83 % | 183 |
| PSR off | **7.613 W** | 17-46 % | 0 |

**0.469 W** saved, ~6 %. The estimate in the previous section (0.7-1.0 W) was
too high but the right order of magnitude, and this is 20x what PC6 -> PC8 was
worth. `DC5 -> DC6` moved off zero for the first time in the whole
investigation, so the DMC really does reach DC6 once scanout stops.

### And the panel goes black

With PSR entered, the screen shows nothing; content reappears only on input.
Sampling the sink over an idle 90 s window (detached, machine untouched -
any terminal animation keeps PSR out of entry entirely and makes the
measurement meaningless):

- source `EDP_PSR_STATUS` = `SRDENT` for 39 of 45 samples - the display engine
  has stopped scanning out,
- sink DPCD `0x2008 PSR_STATUS` = `INACTIVE` for **all** samples,
- sink DPCD `0x2006 PSR_ERROR_STATUS` = `0x00` throughout - no CRC error, no
  RFB storage error, no SDP error.

So the source enters self-refresh while the panel never does. Nothing drives
the panel and it goes black. The black screen corroborates the register
reading independently: had the panel entered PSR and merely reported it wrong,
the visible result would be a frozen frame, not black.

Beware of a wrong decode here: for PSR1 `psr_source_status()` uses the
*second* `live_status[]` table (`IDLE, SRDONACK, SRDENT, BUFOFF, BUFON,
AUXACK, SRDOFFACK`), not the PSR2 one. State 2 is `SRDENT`, not `CAPTURE_FS`.

### Six configurations tried live, all failed

`EDP_PSR_CTL` is at MMIO `0x6f800` (TRANSCODER_EDP) in BAR0 of `00:02.0`,
mmap-able from userspace via `resource0`; sink DPCD is writable through
`/dev/drm_dp_aux0`. That gives a full experiment loop with **no rebuild and no
reboot** - worth remembering for any future display-register work.

Decoded stock `EDP_PSR_CTL` = `0x81f006e9`: enable, LINK_STANDBY **off**,
MAX_SLEEP_TIME 31, CRC_ENABLE, TP1/TP2 2500us, IDLE_FRAMES 9.

| | change | sink entered PSR |
|---|---|---|
| A | stock, as the driver configures it | no |
| B | `DP_PSR_MAIN_LINK_ACTIVE` + `EDP_PSR_LINK_STANDBY` | no (source went to an undefined state 7) |
| C | CRC verification off, source and sink | no |
| D | standby + no CRC | no |
| E | `DP_PSR_FRAME_CAPTURE` set | no |
| F | frame capture + CRC + IDLE_FRAMES 15 | no |

`link_standby` being 0 is not an Apple decision, it is just an unpopulated VBT
(`psr.full_link` defaults to 0, and it is only read at all for
DISPLAY_VER < 12). Forcing it did not help, so the failure is in the entry
handshake itself, not in how the link is powered down.

The panel advertises `DP_PSR_NO_TRAIN_ON_EXIT` and a 330 us setup time
(`PSR_CAPS = 0x01`), which is the benign case on paper, and
`intel_psr_entry_setup_frames()` accepts it. It still never acknowledges
entry. Whatever the "additional handling" in the quirk's comment refers to, it
is not reachable through the standard PSR knobs.

### What would still be worth trying, if anyone picks this up

Not attempted, in rough order of promise:

- capture the eDP link with the panel driven by macOS to see what Apple's
  driver sends at PSR entry - this is the only way to learn the missing step,
  and it needs hardware we do not have,
- check whether newer panels in other T2 models behave differently before
  concluding anything about "Apple panels" as a class.

Narrowing the upstream wildcard to specific device ids is **not** supported by
this data: this panel (`65 44 13 67 64 61`) is a case where the wildcard is
correct.

### Machine-specific facts worth keeping

- `CONFIG_MODVERSIONS` is not set on Fedora 44, Secure Boot is off, so a single
  in-tree module can be rebuilt out-of-tree against `kernel-devel` and dropped
  in next to the stock ones. Upstream `linux-7.1.9` sources match Fedora's for
  `drivers/gpu/drm/display/` - verified by building unmodified first and
  diffing sections and exports against the shipped `.ko` (only `.text` differed,
  by 201 bytes, from the gcc 16.2.1 vs 16.1.1 mismatch).
- **Compress replacement modules with `xz --check=crc32 --lzma2=dict=1MiB`**,
  the same options `scripts/Makefile.modinst` uses. Plain `xz -9` produces a
  file the in-kernel decompressor rejects with
  `decompression failed with status 6`; the module then does not load, i915
  never probes, and the desktop comes up on `simple-framebuffer` with software
  rendering. That failure looks exactly like "PSR made the graphics slow and
  reset my scaling" and cost one reboot to diagnose.
- The GRUB line editor is unusable on T2 Macs (see
  `local/learnings/installer/grub-menu.md`), so the plan's escape hatch of typing
  `i915.enable_psr=0` at boot does not exist here. Any boot-time fallback has
  to be its own BLS entry in `/boot/loader/entries/`, and
  `grub2-editenv - unset menu_auto_hide` is needed to make the menu visible at
  all after a successful boot.

## Source-OUI unlock hypothesis for Apple PSR: ruled out from userspace (2026-08-23)

Follow-up to *"PSR quirk lifted and measured"*. The KaiT2en project owner
suggested the panel may simply be missing a handshake: macOS programs the
**source OUI** at DPCD 0x300-0x302 to unlock advanced panel functions, so the
Apple TCON might gate its PSR state machine on seeing Apple's OUI rather than
Intel's. The mechanism is real and i915 uses it itself, but on this panel it
is not what blocks PSR.

### Why the hypothesis was worth testing

`intel_dp_init_source_oui()` in `intel_dp.c` writes Intel's OUI
`{0x00, 0xaa, 0x01}` to `DP_SOURCE_OUI`, called from `intel_dp_set_power()`
under the comment *"Write the source OUI as early as possible"*, i.e. right
before `DP_SET_POWER_D0`. Upstream explicitly knows this register gates sink
behaviour:

> During driver init, we want to be careful and avoid changing the source OUI
> if it's already set to what we want, so as to avoid clearing any state by
> accident

There is also an in-tree precedent: Intel's DPCD HDR backlight interface is
unlocked exactly by writing Intel's OUI. A panel gating PSR on a vendor OUI
would be the same mechanism with a different vendor.

### What this machine actually reports

    DPCD 0x300 SOURCE_IEEE_OUI  = 00 aa 01   (Intel, written by i915)
    DPCD 0x303 SOURCE_DEVICE_ID = 00 00 00 00 00 00   (i915 never writes it)
    DPCD 0x400 SINK_IEEE_OUI    = 00 10 fa   (Apple, matches the quirk row)
    DPCD 0x403 SINK_DEVICE_ID   = 65 44 13 67 64 61
    DPCD 0x070 PSR_SUPPORT      = 01 01      (PSR1, link training on exit not required)

### Linux-side AUX trace of a PSR enable

A kprobe on `drm_dp_dpcd_access` (args: `req=%si` 8=native write / 9=native
read, `off=%dx`, `size=%r8`, buffer bytes via `+0(%cx)`) gives a complete
DPCD trace with no rebuild. The entire sink-side PSR enable sequence i915
performs is two byte writes to `DP_PSR_EN_CFG`:

    req=8 off=0x170 size=1 b0=0x04      DP_PSR_CRC_VERIFICATION
    req=8 off=0x170 size=1 b0=0x05      + DP_PSR_ENABLE

No vendor registers, no device ID, no preparatory sequence. Everything else
(VSC SDP) goes in-band over the main link, not over AUX.

### Methodology trap: sampling without confirming source entry

The first run wrote the Apple OUI and sampled the sink while the GNOME
desktop kept repainting. The source therefore never left `IDLE` and the sink
was never asked to do anything — the "sink stays INACTIVE" result was
meaningless. **Any PSR sink test must first poll
`Source PSR/PanelReplay status` until the state field (`raw >> 29 & 7`)
leaves IDLE, and only then sample the sink; otherwise report INCONCLUSIVE.**
Switching to a text console with `setterm --cursor off` makes the frontbuffer
genuinely static and the source enters `SRDENT` within 0.2 s every time.

### Results

With source entry confirmed (`SRDENT`, raw `0x40010009`, after 0.2 s), the
sink stayed `INACTIVE` with `PSR_ERROR_STATUS = 0x00` in every configuration:

| # | source OUI | source device ID | sink 0x2008 |
|---|-----------|------------------|-------------|
| P0 | 00 aa 01 (Intel) | zeros | INACTIVE |
| P1 | 00 10 fa (Apple) | zeros | INACTIVE |
| P2 | 00 10 fa | "Apple" | INACTIVE |
| P3 | 00 10 fa | "Apple" + hw rev 0x01 | INACTIVE |

Identical raw source status in all four. Writing the OUI at runtime changes
nothing.

### The remaining variant is not testable from userspace

macOS writes the OUI *before* the panel is powered up, so the ordering
matters. Driving the panel through a real D3 -> D0 cycle via the GNOME
screensaver DBus interface (`org.gnome.ScreenSaver SetActive`, verified
against `/sys/class/drm/card1-eDP-1/dpms`) showed:

- the panel **clears** `DP_SOURCE_OUI` to `00 00 00` while in D3 — the
  register is volatile;
- writing `00 10 fa` while in D3 works and reads back;
- but after wake it reads `00 aa 01` again: **i915 rewrites Intel's OUI on
  power-on and always wins**, before the link is up.

So `oui_valid` does *not* protect a userspace-written OUI across a power
cycle on this kernel, and no userspace sequence can make Apple's OUI be the
one present when the panel comes up.

**Conclusion:** the only way to settle the hypothesis is to change the
constant in `intel_dp_init_source_oui()` to `{0x00, 0x10, 0xfa}` and rebuild
i915. That also requires rebuilding `drm_display_helper` with the quirk
lifted in the same boot, plus a rollback BLS entry. Until someone does that
— or captures the eDP AUX traffic under macOS with DTrace `fbt` (macOS is
installed on `nvme0n1p2`, but it needs SIP disabled) — the OUI theory is
neither confirmed nor refuted; only its userspace-reachable forms are ruled
out.

### Reusable technique

`org.gnome.ScreenSaver SetActive` is a reliable way to force a real panel
D3 -> D0 cycle from a root script without DRM master, verifiable via
`/sys/class/drm/card1-eDP-1/dpms`. Note `lock-delay` is 0 here, so
`org.gnome.desktop.screensaver lock-enabled` must be temporarily set false
and restored, or the test locks the session. `setterm --blank force` on a VT
does **not** power the panel down (dpms stays `On`), and this kernel exposes
no `/sys/class/graphics/fb0`, so the fbdev blank path is unavailable.

## Source-OUI hypothesis settled by patching i915: refuted (2026-08-24)

The userspace round (previous section) could not test the ordering that
matters, because i915 rewrites `DP_SOURCE_OUI` at panel power-on and always
wins. Settled it by patching the driver.

### Patch

Two one-liners against vanilla 7.1.9 built with the Fedora config:

- `drm_dp_helper.c`: Apple wildcard row `BIT(DP_DPCD_QUIRK_NO_PSR)` -> `0`
- `intel_dp.c`: `intel_dp_init_source_oui()` gets a runtime-switchable OUI
  behind a new `i915.t2_source_oui` module parameter (0=Intel `00 aa 01`,
  1=Apple `00 10 fa`, 2=Apple + `SOURCE_DEVICE_ID "Apple"`), plus a
  `drm_info()` marker showing what the panel held and what was written.

The parameter is mode 0644, and a DPMS off/on cycle re-runs the function, so
all three variants can be A/B tested in one boot with no rebuild.

### Result: negative, and this time under the right conditions

At boot, before the link was up:

    i915 0000:00:02.0: [drm] t2_source_oui=1: panel had 00 00 00, writing 00 10 fa

| mode | OUI in panel at test time | source | sink 0x2008 |
|------|---------------------------|--------|-------------|
| 1 Apple | `00 10 fa` | SRDENT `0x40010009` @0.2s | INACTIVE x27 |
| 2 Apple + device ID | `00 10 fa` + `"Apple"` | SRDENT `0x40010009` @0.2s | INACTIVE x27 |
| 0 Intel (control) | `00 aa 01` | SRDENT `0x40010009` @0.2s | INACTIVE x27 |

Bit-identical raw source status, `PSR_ERROR_STATUS` 0x00 throughout. **The
panel behaves exactly the same no matter whose source OUI it sees, including
when Apple's OUI is present from the very first panel power-on.** The
source-OUI unlock theory is refuted for the OUI and device-ID form of it. It
remains conceivable that macOS writes additional vendor registers; only a
macOS-side AUX capture could show that.

### Build gotcha that cost one boot: struct module size

Building the modules with `--disable DEBUG_INFO_BTF DEBUG_INFO_BTF_MODULES`
produced modules the kernel refused at load time:

    module drm_display_helper: .gnu.linkonce.this_module section size must
    match the kernel's built struct module size at run time

`CONFIG_DEBUG_INFO_BTF_MODULES` adds fields to `struct module`, so turning it
off shrank `.gnu.linkonce.this_module` from 1344 to 1280 bytes. **vermagic
matched, so nothing caught it until the module was actually loaded** — and
because i915 depends on drm_display_helper, the machine came up on
`simpledrm` with software rendering.

Two rules follow:

1. Build replacement modules with the **unmodified** Fedora config. The only
   safe change is `--disable MODULE_SIG_ALL` (it does not touch
   `struct module`). BTF generation then prints *"Skipping BTF generation ...
   due to unavailability of vmlinux"* and that is harmless — BTF is optional
   metadata, not needed to load.
2. The equivalence check must compare **`.gnu.linkonce.this_module`
   explicitly**, not just vermagic and exported symbols. It differed in the
   first check and was not acted on.

Best verification short of rebooting: `modprobe` the rebuilt helper into the
running kernel. It loads or it does not, and that answers the ABI question in
one second.

### Other build notes

- `M=` builds against a self-extracted tree need the base kernel's
  `Module.symvers` copied from `/lib/modules/$(uname -r)/build/`, otherwise
  modpost fails with hundreds of "undefined!" symbols.
- `echo "-200.fc44.x86_64" > localversion` in the tree root makes
  `make kernelrelease` produce the Fedora release string, which is what
  vermagic needs.
- Acceptable residual diffs vs the shipped module: missing `.BTF` and
  `.gnu_debuglink`, extra `.comment` / `.note.GNU-stack`, and `.modinfo`
  differing by the signature entry.

## macOS AUX trace: the missing piece is a vendor register, not the OUI (2026-08-24)

Traced macOS 15.7.7 (build 24G720) on the same machine with DTrace/fbt on
`AppleIntelPort::readAUX`/`writeAUX`, two full display sleep/wake cycles plus 65 s
idle. Raw logs and the full report are on the ESP under `/boot/efi/psr-trace/`
(`macos-result.md`, `aux-*.log`). Findings below were re-verified against the raw
logs, not just taken from the report's prose.

### macOS does use PSR on this panel, at 100 % idle residency

`powermetrics` exposes an undocumented IOReport channel `"Panel Self Refresh State"`
(channel id 0x505352 = "PSR") in the `AppleIntelICLLPGraphicsFramebuffer` node.
Over 65 s idle: `Only TCON is On = 100.00%`, `StateMachine FIFO = 100.00%`. In that
same window the entire AUX bus carried exactly two transactions, both backlight
values on 0x722. So self-refresh is sustained with zero AUX keepalive traffic.

### The source-OUI theory is dead from the other side too

macOS never writes and never reads 0x300-0x30B. Not once, in either run, at any
point of panel power-on or PSR enable. The only write anywhere in 0x300-0x3FF is a
single vendor register `0x321 <- 0x3c`, 0.5 ms before the PSR enable. This
independently confirms the Linux-side result: the OUI is not what unlocks PSR.

### The panel is an Apple "Banksia" TCON reached through a DPCD paging window

`BanksiaTcon`/`CamelliaBase` are compiled into the framebuffer kext (no separate
kext, no IOService node); the live instance was confirmed by vtable symbolisation.
`CamelliaBase::WriteCamelliaReg(reg, size, val)` is layered on plain DPCD AUX:

  reg < 0x01000000            -> direct DPCD access at that address
  reg = 0x01_BB_OOOO          -> DPCD[0x4E0..0x4E1] <- { BB, OOOO & 0xF0 }   selector
                                 DPCD[0x4F0 + (OOOO & 0x0F)] <- value        data window

Decoding verified mechanically against every indirect access, 16/16 in each run.

### 0x4D4 is the actual self-refresh trigger, and i915 never writes it

macOS writes the standard `0x170 <- 0x01` once at panel power-on (note: value 0x01,
DP_PSR_ENABLE only, *without* DP_PSR_CRC_VERIFICATION -- i915 writes 0x04 then 0x05).
But the panel does not latch a frame until, ~5 s later on the `DoDelayEnablePsrTimer`:

  DPCD 0x4D4 <- 0x01    enter self-refresh   (between FIFOEnter() and SRDEnter(), us apart)
  DPCD 0x4D4 <- 0x02    exit self-refresh    (at FIFOExit(), on display sleep)

Bit-identical timing in both runs. 0x4D4 is in the sink block and undefined in the
DisplayPort spec. This explains the Linux symptom exactly: source reaches SRDENT,
sink stays INACTIVE, error register stays 0x00 -- the sink was never told to capture.

Crucially it is a one-shot per power cycle, not a per-entry handshake: 0x4D4 appears
exactly twice in a 45 s window that included active desktop use, and not at all
during 65 s of sustained self-refresh. That makes it implementable at PSR enable time.

The full arming block macOS issues, in order, immediately before the 0x170 write:

  0x321 <- 0x3c
  0x4D2 <- 0x01
  0x4D1 <- 0x03
  0x170 <- 0x01

macOS never reads 0x2008 (PSR_STATUS) at all; it tracks state via TCON interrupts.

### Confirmed reachable from Linux userspace (stock kernel, read-only probe)

/dev/drm_dp_aux0 is the eDP panel: DPCD 0x000 reads `11 0a 84 41`, bit-identical to
the macOS trace. All vendor addresses respond with plausible state:
0x420=0x89, 0x4CC=0x02, 0x4D1=0x07, 0x4D2=0x00, 0x4D4=0x00, 0x321=0x00.
Sink OUI at 0x400 = `00 10 fa` + device id `65 44 13 67 64 61`.

The TCON paging window works from Linux and returns the same values as macOS:
selector {09, c0} then read 0x4F0 gives 0x85, exactly as the macOS trace recorded for
reg=0x010900c0. So the whole transport is usable without any kernel change.

Sidebar: macOS trains this link at LINK_BW_SET 0x0c (3.24 Gbps) although the panel
advertises MAX_LINK_RATE 0x0a. This is not a mystery -- upstream already has
DP_DPCD_QUIRK_CAN_DO_MAX_LINK_RATE_3_24_GBPS, but only for the sibling Apple panel
with device id `65 44 15 65 62 61` (drm_dp_helper.c). Ours has `65 44 13 67 64 61`
and therefore falls into the DEVICE_ID_ANY entry instead. Unrelated to PSR.

## Replaying the Apple PSR sequence from userspace: partial success (2026-08-24)

Setup: rebuilt i915 with the Apple entry in the `drm_dp_helper.c` quirk table reduced
to `0` (NO_PSR removed), `i915.t2_source_oui=0` so the OUI stays Intel's and only one
variable changes. Tests run on VT3 with the cursor off, because on the GNOME desktop
the source never leaves IDLE.

Build note that cost a boot: kernel modules must be compressed the way Fedora does it,
`xz --check=crc32 --lzma2=dict=1MiB`. A plain `xz -T0` defaults to CRC64, which the
in-kernel XZ decoder rejects with `decompression failed with status 6` -- and i915 then
silently never loads, leaving the machine on simpledrm with no i915 message in dmesg at
all. `xz -t` in userspace passes such a file happily, so it is not a useful check;
compare `xz --list -vv` against the stock module instead.

### What now works

* Quirk lift is effective: `Sink support: PSR = yes [0x01]`, `PSR mode: PSR1 enabled`,
  source reaches `SRDENT [0x40010000]` on an idle console within 0.3 s.
* The TCON paging window works and the panel is in the same state macOS finds it in.
  Read back through the window, identical to the macOS trace: `[02:e9]=0x58`,
  `[09:c0]=0x85`, `[06:02]=0x00`. Writes to `0x321`, `0x4D1`, `0x4D2`, `0x170`, `0x4CC`
  all read back correctly.
* `0x4D4` reads back as `0x00` immediately after writing `0x01`, so it is write-only or
  self-clearing -- consistent with it being a command register rather than a state bit.
* `0x420` and `0x2006` read back `0x00` after writing `0xff`: both write-1-to-clear.

### The panel reacts for the first time

Writing `0x4D4 <- 0x01` while the source sits in SRDENT made `DP_PSR_ERROR_STATUS`
(0x2006) go from `0x00` to `0x02` = `DP_PSR_RFB_STORAGE_ERROR`. Every earlier experiment
left that register at `0x00`. So the sink is no longer silent: it now tries to store the
remote frame buffer and fails. That is progress, not a regression.

### Why the full replay still does not capture

Replaying the entire macOS sequence (InitTconHW block, arming block, post-arming TCON
block, then `0x4D4 <- 0x01`) still leaves `0x2008` at `0x00`, and ends with i915 turning
PSR off by itself:

    PSR mode: disabled
    PSR sink not reliable: yes

`intel_psr_short_pulse()` reads 0x2006, sees the RFB storage error, sets
`psr.sink_not_reliable` and disables PSR. The flag is sticky -- toggling
`i915_edp_psr_debug` does not clear it; it is only reset when the DPCD is re-read at
connector init, i.e. effectively a reboot.

Two things cannot be reproduced from userspace, and they are the likely blockers:

1. **Timing.** macOS issues the whole TCON init during panel power-on, *before* link
   training (`hwSetPanelPower(2)` -> InitTconHW -> arming -> LINK_BW_SET). We can only
   replay it on an already-trained link, where the TCON has presumably latched a
   different configuration. Consistent with this: macOS reads `[04:f0]` as
   `00 0a 05 08` right after writing `[04:ff] <- 0x00`, whereas here the same read gives
   `00 00 00 00`.
2. **i915 fights back.** Apple's driver never reads 0x2008 and handles errors through
   TCON interrupts; i915 polls the standard error register and gives up permanently on
   the first error.

### Conclusion

The remaining work belongs in the driver, not in userspace: the Apple sequence has to be
issued from the panel power-on / `intel_psr_enable_sink()` path, and i915's error
handling has to tolerate `RFB_STORAGE_ERROR` during the handshake instead of latching
`sink_not_reliable`.

## PSR works on the Apple panel: the fix, and what it is worth (2026-08-24)

Panel Self Refresh is now running on the internal eDP panel of this MacBookPro16,2.
`DP_PSR_STATUS` (0x2008) reports `0x03` = `DP_PSR_SINK_ACTIVE_SINK_SYNCED` -- the panel
displays from its own remote frame buffer on its own timing -- held stably with
`DP_PSR_ERROR_STATUS` at `0x00` throughout.

### The minimal sequence

Only four DPCD writes are needed beyond what i915 already does, all issued from
`_psr_enable_sink()`:

    0x321 <- 0x3c      vendor
    0x4D2 <- 0x01      vendor
    0x4D1 <- 0x03      vendor
    0x170 <- 0x01      DP_PSR_ENABLE, WITHOUT DP_PSR_CRC_VERIFICATION
    0x4D4 <- 0x01      vendor: capture trigger

The full `InitTconHW` block from the macOS trace (paging window at 0x4E0/0x4F0, banks
02/04/06/09, plus 0x4CC and 0x420) was implemented and tested as well and makes **no
difference at all**: identical sink status, identical error register. It was removed
again. The TCON paging window is real and reads back the same values macOS sees
(`[02:e9]=0x58`, `[09:c0]=0x85`), but reproducing macOS's TCON configuration is not
required to get self-refresh -- only the trigger is.

Likewise, tolerating `DP_PSR_RFB_STORAGE_ERROR` in `intel_psr_short_pulse()` was needed
while poking the panel from userspace, but with the correct sequence no error is ever
raised, so that hack was removed too.

### The second half of the fix: the VBT

Lifting the quirk and adding the handshake is not enough -- PSR still never came up on
its own. `psr_global_enabled()` returns, for eDP, nothing but `connector->panel.vbt.psr.enable`,
and in this kernel it does not consult `display->params.enable_psr` at all in the
`I915_PSR_DEBUG_DEFAULT` case. So `i915.enable_psr=1` cannot help. Apple's VBT does not
set the PSR enable bit (their own driver does not use Intel's VBT path), hence
`PSR disabled by flag` at every modeset. The fix returns true for Apple TCON panels.

### Measured saving

A/B/A/B on an idle text console, 30 s per phase, PSR forced on and off between phases
with everything else identical:

    phase      RAPL package     battery
    PSR on       1.117 W        5.873 W
    PSR off      1.287 W        6.029 W
    PSR on       1.120 W        5.884 W
    PSR off      1.288 W        6.021 W

    delta        0.169 W        0.146 W

Repeats agree to within 0.003 W, so the effect is well outside the noise.

**Do not take this as the real-world figure -- it is the weakest case.** A text console
has little display pipeline to idle. Measured again on an idle GNOME desktop (see below)
the saving is more than three times larger. The text-console number is kept here only
because it was the first clean A/B.

Caveat on the state reached: the sink reports `0x03` (sink-synced, main link still up),
whereas macOS reports `Only TCON is On = 100%`, i.e. the display engine off as well.
There may be more to gain by getting the main link down, but that is a separate question
from making PSR work at all.

### Build/config notes

* `t2_apple_psr` (int, 0644, default 1) gates the whole thing; keyed on sink OUI
  `00:10:fa` from DPCD 0x400 so no other panel is affected.
* Iterating is cheap: the parameter is runtime-writable and writing
  `i915_edp_psr_debug` forces a modeset, so `_psr_enable_sink()` re-runs without a reboot.
  `psr_debug` 1 = force off, 2 = force PSR1, 0 = back to auto.
* On the GNOME desktop the source sits in IDLE because mutter repaints constantly; PSR
  entry is only observable on an idle VT (`chvt 3`, `setterm --cursor off`).

### Real-world saving on an idle GNOME desktop

The measurement that actually matters. Same A/B method, 30 s per phase, three
on/off pairs, run fully detached so no terminal output could repaint the screen (an
earlier attempt was contaminated exactly that way -- any output from the controlling
session forces PSR out and biases the result low):

    phase      RAPL package     battery      SRDENT residency
    PSR on       0.776 W        6.294 W          15/15
    PSR off      1.132 W        6.749 W           0/15
    PSR on       0.775 W        6.255 W          15/15
    PSR off      1.190 W        6.817 W           0/15
    PSR on       0.749 W        6.213 W          15/15
    PSR off      1.119 W        6.742 W           0/15

    mean         on 0.767 W / off 1.147 W        on 6.254 W / off 6.769 W
    delta            0.380 W                          0.515 W

So the panel returns **~0.38 W package, ~0.52 W at the battery** -- at or slightly above
the 0.47 W originally estimated from PC-state residency, and roughly 3x what the text
console showed. The battery delta exceeding the package delta is expected: it includes
VRM losses and the link/panel side, which RAPL does not see.

Two things worth recording:

* **PSR reaches 100 % residency on an idle GNOME desktop** (15/15 samples in SRDENT in
  every on-phase, 0/15 in every off-phase). The earlier worry that mutter repaints too
  often for PSR to ever engage was unfounded -- a genuinely idle desktop is idle enough.
* Discard any power phase whose repeats disagree by more than ~0.05 W. One early off-phase
  read 1.573 W against 1.119-1.190 W for the same configuration; that was background load,
  not PSR.

### Suspend/resume

Verified with a detached script across a real `systemctl suspend` cycle (50 s asleep):

    before suspend   PSR1 enabled   src=SRDENT   sink=03   err=00
    after resume     PSR1 enabled   src=IDLE     sink=03   err=00
    idle console     PSR1 enabled   src=SRDENT   sink=03   err=00   SRDENT 20/20

The handshake re-arms itself: `_psr_enable_sink()` runs on the resume modeset like on any
other, so the vendor writes and the 0x4d4 trigger are reissued without any extra plumbing.
No error is ever reported. The `IDLE` right after resume is just the desktop repainting.

Worth noting because it was a live concern beforehand: macOS defers its own trigger by 5 s
via `DoDelayEnablePsrTimer`, which suggested the timing might matter. On Linux it does not
-- issuing it inline from the modeset path is enough.

### The "can the main link go down too?" question: closed, nothing to gain

Raised as an open side question after seeing macOS report `Only TCON is On = 100%` while
our sink reports `0x03`. Investigated and closed the same day; the premise was wrong.

* **The main link is already being shut down.** `EDP_PSR_LINK_STANDBY` (bit 27 of PSR CTL)
  reads 0 in `0x81f006e9`, i.e. i915 is not using link-standby mode -- it powers the main
  link down during self-refresh. There was never anything to fix here.
* **PSR is what unlocks DC5.** Measured over idle phases: `DC3 -> DC5` increments only
  while PSR is on (+1 per 25 s phase, meaning it enters once and stays), and never
  increments with PSR off. So the display engine's deep state is already a consequence of
  the PSR work.
* **DC6 is out of reach, and that is a hardware limit.** It never increments during PSR
  idle. On this generation DC6 with the panel lit needs PSR2, and this panel advertises
  PSR1 only: DPCD 0x070 reads `0x01`, Panel Replay is `no`. The 195 historical
  `DC5 -> DC6` transitions come from panel-off situations (blank/suspend), not from PSR.
* **The macOS comparison was an over-reading.** `Only TCON is On` is Apple's own state
  machine accounting from their IOReport channel, not an Intel DC state. It does not imply
  a deeper hardware state than what we reach, and macOS runs the same PSR1-only panel.

Conclusion: the 0.38 W package / 0.52 W battery already measured is what this panel has to
give. Do not reopen this without new evidence.

## Two more machines confirm the handshake, and upstream already has the right hook (2026-08-25)

Test kernels with both patches were handed to KaiT2en testers. Two independent reports came
back, both showing real self-refresh (`DPCD 0x2008 = 03`, `0x2006 = 00`, high `SRDENT`
residency on an idle desktop).

### The three machines measured so far

| Machine | Panel | Sink device_id (DPCD 0x403..0x408) | hw/sw rev |
|---|---|---|---|
| MacBookPro16,2 (Ice Lake, 13") | ours | `65 44 13 67 64 61` | `01 06 03` |
| MacBookPro15,1 (Coffee Lake, 15", dual GPU) | tester | `65 44 15 67 62 61` | `01 08 04` |
| MacBookAir9,1 (Ice Lake, 13") | tester | `65 44 13 68 64 61` | `01 09 02` |
| *(2017 15", already in upstream's quirk table)* | -- | `65 44 15 65 62 61` | -- |

Three distinct panels, two chipset generations, three different TCON firmware revisions --
all take the identical five-write sequence. The handshake is an Apple TCON family property,
not a property of one panel.

Every machine reports the same vendor register state: `0x420 = 89`, `0x4cc = 02` before the
patch, and `0x321 = 3c`, `0x4d1 = 03`, `0x4d2 = 01` after it. `0x070 = 0101` (PSR1, no
Panel Replay) everywhere.

### Device ID structure (observed, not documented)

Read as ASCII the six bytes are `"eD" <size> <rev> <rev> 'a'`. Byte 3 tracks the display
diagonal in inches written as hex digits: `0x13` on both 13" machines, `0x15` on both 15"
machines, including the 2017 panel already known to upstream. Byte 5 tracks the same split
(`'d'` on 13", `'b'` on 15"). Byte 4 appears to be a supplier/revision letter -- the two
Ice Lake 13" panels differ only there (`0x67` vs `0x68`).

Useful for sanity-checking tester reports; not something to depend on.

### There is no endianness in this field

Raised in review: whether `device_id` needs a byte swap. It does not. `struct
drm_dp_dpcd_ident` declares `u8 device_id[6]` and both quirk matchers compare it with plain
`memcmp()` against the DPCD read order. The existing upstream Apple entry
`DEVICE_ID(101, 68, 21, 101, 98, 97)` is `65 44 15 65 62 61` -- read order, no swap.

### i915 already has a sink-keyed quirk table; use it, not DMI

`intel_quirks.c` carries `intel_dpcd_quirks[]`, matched in `intel_init_dpcd_quirks()` on:

* sink OUI (mandatory `memcmp`),
* sink device_id (**skipped when the table entry leaves it zero** -- `mem_is_zero`),
* PCI device / subsystem vendor / subsystem device (`DEVICE_ID_ANY` / `PCI_ANY_ID` allowed).

So one table supports both "every panel from this vendor" and "exactly these panels", and
narrowing later is a one-line edit. `quirk_disable_edp_panel_replay` is already in there,
gated on sink OUI alone -- direct precedent for gating a PSR-family feature this way.

DMI matching is the wrong key here, for a reason the KaiT2en Project Owner supplied himself:
the A1708 (13" 2016/2017, T1) and the A1990 (15" 2018, T2) carry the same TCON board. One
panel, two chassis, two Mac generations -- a DMI table needs an entry per machine for a
property that belongs to the panel, and gets it wrong in the other direction too, since
Apple mixed panel suppliers inside a single model.

### Existing safety layers, in case a panel does not like the sequence

1. `_psr_enable_sink()` is only reached when the panel advertises PSR in DPCD `0x070`, so
   pre-PSR Apple panels never see the writes.
2. `i915.enable_psr=0` disables PSR outright (`_psr_compute_config()`), so testers have a
   documented off switch without a custom module parameter.
3. `intel_psr_short_pulse()` already sets `sink_not_reliable` and drops PSR when the sink
   reports errors, so a panel that accepts the writes but misbehaves degrades to today's
   behaviour rather than showing corruption.
4. There is upstream precedent for a quirk that exists purely because the VBT lies about a
   feature (`quirk_backlight_present`: "Some VBT's incorrectly indicate no backlight is
   present"). Our VBT override is the PSR equivalent of that.

### What is still missing

Only successes so far. Without a single Apple panel that *fails* the handshake, the boundary
of the working set is unknown, and a whitelist built from successes alone would just be a
list of who happened to own a test machine. Failure reports are worth more than further
confirmations. Specifically wanted: 16" models (expect `0x16` in byte 3), T1 machines, and
any Intel Mac old enough to advertise PSR but predate this TCON family.

### MacBookPro16,1: not a handshake failure, the panel is not on Intel (2026-08-25)

First non-working report. Reading it correctly matters, because it is not evidence against
the sequence.

```
gpu:    00:02.0 Intel CoffeeLake-H GT2 [UHD Graphics 630]
gpu:    03:00.0 AMD Navi 14 [Radeon RX 5500M]
cand:   /dev/drm_dp_aux3 -> card2-eDP-1  dpcd000=1214c441
  070 0100   400 0010fa65441567626101090a
  321 00     4d1 07    4d2 00    4d4 00    2006 00    2008 00
grep: /sys/kernel/debug/dri/0000:00:02.0/i915_edp_psr_status: No such device
SRDENT: 0/15
```

`i915_edp_psr_status_show()` returns `-ENODEV` only when `HAS_PSR()` is false (impossible on
Coffee Lake) or when `for_each_intel_encoder_with_psr()` finds nothing. So **i915 has no eDP
encoder on this machine at all** -- the internal panel is driven by amdgpu (`card2-eDP-1`),
which the tester confirmed. Corroborating: only one eDP candidate instead of the 15,1's two,
and `dpcd000 = 1214c441` (DP 1.2, HBR2, 4 lanes) rather than the `110a8441` (DP 1.1, RBR)
seen on every Intel-driven panel -- the bandwidth the 16" 3072x1920 panel needs.

The vendor registers are at factory defaults (`0x321 = 00`, `0x4d1 = 07`, `0x4d2 = 00`), i.e.
`_psr_enable_sink()` never ran. The patch is inert here, not broken. PSR on this machine is
an amdgpu question.

**The size hypothesis is refuted.** A 16" machine reports `0x15` in byte 3. Apple carried the
15" designation forward into the 16" line. Do not use byte 3 for anything.

**The device_id is not unique, which kills the whitelist idea.** This panel reports
`65 44 15 67 62 61` -- identical to the working MacBookPro15,1 -- while being a demonstrably
different panel: different DPCD revision, different max link rate, `0x071 = 00` instead of
`01`, firmware `sw 09 0a` instead of `08 04`. A table keyed on OUI + device_id cannot
separate them. This settles the design question in favour of matching the OUI and relying on
the existing runtime safeguards (`0x070` gating, `sink_not_reliable`, `i915.enable_psr=0`).

Test-method note: a blinking text cursor prevents PSR entry and will read as `SRDENT: 0`.
Irrelevant in this report, since PSR was never active, but a real false-negative source if
anyone runs the probe on a VT rather than a desktop.

### MacBookPro16,1 retested on the iGPU: works, and it is a fourth distinct panel (2026-08-26)

Same machine, rebooted with the Intel GPU as primary display adapter. The eDP encoder now
shows up on i915 (`card1-eDP-1`, aux0; the AMD side becomes the silent `card2-eDP-2`), the
handshake runs, and PSR reaches the best residency measured on any machine so far:

```
  321 3c    4d1 03    4d2 01    2006 00    2008 03
  PSR mode: PSR1 enabled
  SRDENT: 15/15
```

This confirms the `ENODEV` reading from the first attempt: nothing was wrong with the panel
or the sequence, i915 simply had no eDP to work with.

Switching is a boot-time choice via the `gpu-power-prefs` EFI variable, already documented
in `howto/06-configuring-gpus.md` ("Set the iGPU as primary display adapter"). Any dual-GPU
tester must do this first, otherwise the probe reports `patch: PATCHED` together with
`i915_edp_psr_status: No such device` -- which reads like a failure and is not one.

### Panels confirmed working, four for four

| Machine | device_id | hw/sw | DPCD 0x000 | 0x071 |
|---|---|---|---|---|
| MacBookPro16,2 (ICL, 13") | `65 44 13 67 64 61` | `01 06 03` | `110a8441` | `01` |
| MacBookPro15,1 (CFL, 15") | `65 44 15 67 62 61` | `01 08 04` | `110a8441` | `01` |
| MacBookAir9,1 (ICL, 13") | `65 44 13 68 64 61` | `01 09 02` | `110a8441` | `01` |
| MacBookPro16,1 (CFL, 16") | `65 44 15 67 62 61` | `01 09 0a` | `1214c441` | `00` |

New in the 16,1: it is the only panel running DP 1.2 / HBR2 rather than DP 1.1 / RBR, and
the only one that does **not** advertise `DP_PSR_NO_TRAIN_ON_EXIT` (`0x071 = 00`). The
handshake works regardless, so it depends on neither the link configuration nor that
capability bit.

Still no case of an Apple panel refusing the sequence. Every non-working report so far has
turned out to be a panel i915 was not driving.

## The upstream series takes shape: v2 patches (2026-08-26)

The two ad-hoc patches were rebuilt as a three-patch series in the form upstream
expects. Files: `v2-0000-cover-letter.patch` .. `v2-0003-*.patch` in the repo
root, generated with `git format-patch`, verified with `git apply` against a
pristine 7.1.9 tree and `checkpatch.pl --strict` (clean; the only warning is
"Unknown commit id", an artifact of the Fedora tarball having no git history).

Series order is dictated by bisect safety. Lifting the `NO_PSR` block first would
create one commit in which i915 drives Apple panels *without* the handshake, i.e.
the broken state. So the machinery lands first, inert:

1. `drm/i915/psr: add Apple eDP TCON PSR handshake quirk` - new
   `QUIRK_APPLE_PSR_HANDSHAKE`, hook, `intel_dpcd_quirks[]` entry, handshake,
   VBT override. No effect yet, `DP_DPCD_QUIRK_NO_PSR` still blocks.
2. `drm/dp: stop disabling PSR on all Apple panels` - deletes the OUI-wide entry.
3. `drm/dp: remove unused DP_DPCD_QUIRK_NO_PSR` - the flag has zero producers
   after 2, and i915 was its only consumer. Split off so it can be dropped if
   the maintainers want to keep the flag.

The 2018 commit is `1035f4a65f58 ("drm/i915: Disable PSR in Apple panels")` by
Jose Roberto de Souza - verified against the actual commit, not reconstructed;
the title is *not* "drm/dp: Add DP_DPCD_QUIRK_NO_PSR" as one might guess from
the flag name.

Deleting the OUI-wide Apple entry is safe for the second Apple entry (the 2017
15" panel with `CAN_DO_MAX_LINK_RATE_3_24_GBPS`): `drm_dp_get_quirks()` iterates
the whole list and ORs every match rather than stopping at the first one
(`drm_dp_helper.c`, `quirks |= quirk->quirks`).

Two deliberate changes against the v1 code that ran on all four test machines:

- The module parameter `t2_apple_psr` is gone. `i915.enable_psr=0` already
  disables PSR (`_psr_compute_config()`), so a private parameter would not
  survive review. Consequence for testers: "is this kernel patched" can no
  longer be answered via `/sys/module/i915/parameters/t2_apple_psr`. Use the
  quirk's own `drm_info` line instead: `dmesg | grep "Apple PSR handshake"`.
- `_psr_enable_sink()` is no longer bypassed by an early return. The handshake is
  woven into the existing path: unlock writes before the generic block,
  `DP_PSR_CRC_VERIFICATION` masked off under the quirk, trigger write at the end.
  **This is untested on hardware.** The generic path writes `DP_PSR_EN_CFG` twice
  (once without `DP_PSR_ENABLE`, once with); v1 wrote it once. On our panels
  `has_sel_update` is false and `link_standby` is false, so `val` should be 0x00
  then 0x01, and the extra 0x00 write lands before the trigger - it *should* be
  equivalent, but that needs a build plus a re-measurement before anything is
  sent. If it regresses, fall back to the v1 shape (early return) and justify it
  in a comment.

Nothing needs to be reset on disable: `intel_psr_disable_locked()` only writes
`DP_PSR_EN_CFG = 0`, the vendor registers stay latched, and the A/B power
measurement toggled PSR three times without re-running the handshake.

## The v2 code shape works on hardware (2026-08-26)

The COPR build of the three-patch v2 series succeeded and the resulting kernel
`7.1.10-200.kait2en.psr.g71437d2.fc44.x86_64` is installed and running on the
MacBookPro16,2. The open question from the previous section - whether the
integrated `_psr_enable_sink()` is equivalent to the v1 early-return shape - is
answered: it is. No fallback is needed, the v1 shape can be dropped for good.

Boot state, nothing done by hand:

```
[    2.439909] i915 0000:00:02.0: [drm] Applying Apple PSR handshake quirk
PSR mode: PSR1 enabled
Source PSR/PanelReplay ctl: enabled [0x81f006e9]
sink 0x2008 = 03    err 0x2006 = 00
```

So the double `DP_PSR_EN_CFG` write (0x00 then 0x01) really is harmless on this
TCON, as predicted: the trigger write to 0x4d4 is what arms it, and it comes
last either way. The sink OUI at DPCD 0x400 reads `00 10 fa`, which is what the
new `intel_dpcd_quirks[]` entry keys on - the quirk hook fires 2.4 s into boot,
before any userspace is up.

Sampling `Source PSR/PanelReplay status` 60 times over 15 s on a live GNOME
desktop gives 14 hits on `SRDENT` and the rest `IDLE`. That is not a
contradiction of the earlier 100 % figure: the earlier measurement ran on a
blank text console with the cursor disabled, this one has a blinking cursor and
a clock repainting. Entry works, which is all this check was for.

Health of the rest of the machine on the new kernel: all DKMS modules rebuilt
for 7.1.10 and loaded (`t2bce_*`, `t2bdrm`, `t2touchbar_*`, `t2smc`, `t2smp`,
`t2hid`, `hid_t2magicmouse`, `t2mfi_fastcharge`), no failed systemd units,
`react-drm` active, both DRM cards present, audio sinks enumerated. `t2gmux`
staying unloaded is correct here - the 16,2 has no dGPU and therefore no
vgaswitcheroo. No i915 or drm errors in dmesg at all.

Still to redo on this kernel: suspend/resume (verified on the v1 shape, not yet
on this one) and the A/B power measurement.

## COPR test round: five machines, four device IDs, zero failures (2026-08-27)

The COPR kernel `7.1.10-200.kait2en.psr.g71437d2.fc44` was handed to the KaiT2en
Discord. Two reports came back, both clean, plus the project owner re-testing his
own two machines on the packaged kernel rather than a self-built one.

| Machine | device_id | hw/sw | quirk | PSR mode | 0x2008 | 0x2006 |
|---|---|---|---|---|---|---|
| MacBookPro16,2 (ICL, 13") | `65 44 13 67 64 61` | `01 06 03` | yes | PSR1 | `03` | `00` |
| MacBookPro16,1 (CFL, 16", iGPU) | `65 44 15 67 62 61` | `01 09 0a` | yes | PSR1 | `03` | `00` |
| MacBookPro15,2 (CFL, 13") | `65 44 13 67 62 61` | `01 05 04` | yes | PSR1 | `03` | `00` |
| MacBookPro15,1 (CFL, 15") | `65 44 15 67 62 61` | `01 08 04` | yes | PSR1 | `03` | `00` |
| MacBookAir9,1 (ICL, 13") | `65 44 13 68 64 61` | `01 09 02` | yes | PSR1 | `03` | `00` |

The 15,2 is a fourth distinct device_id and the oldest firmware seen so far
(`sw 05 04`). Byte 3 is `0x13` on the 13" machines and `0x15` on the 15"/16"
ones, but that is a panel-family marker, not a size marker (the 16,1 is a 16"
reporting `0x15`), and bytes 4..6 vary independently. No usable structure.

### The filter decision, settled by the coupling to patch 2

Narrowing patch 1 to a device_id whitelist looks conservative and is not, because
patch 2 deletes the OUI-wide `DP_DPCD_QUIRK_NO_PSR` entry unconditionally. A
whitelist would therefore split Apple panels into three groups instead of two:
whitelisted panels get the handshake, and every other Apple panel loses the 2018
block while gaining nothing - i.e. plain i915 PSR with `DP_PSR_CRC_VERIFICATION`
set, which is exactly the state commit 1035f4a65f58 disabled as broken. The VBT
usually saves them (`vbt.psr.enable` is clear on these machines, and only the
quirk overrides it), but the 2018 commit exists precisely because at least one
Apple machine did reach PSR enablement, so that is not a guarantee.

The conservative shape is therefore either

* OUI-wide quirk + drop `NO_PSR` (one behaviour for all Apple panels), or
* device_id whitelist + **keep** `NO_PSR`, bypassed only for whitelisted panels
  (untested panels keep today's behaviour bit for bit, patch 3 disappears).

There is no third option that both narrows the match and keeps the blast radius
of the series equal to the blast radius of the tested set.

Going with the first. Reasons, in order of weight: `_psr_init_dpcd()` bails on
`DP_PSR_SUPPORT` before quirks are consulted, so the vendor writes can only ever
reach an Apple panel that already advertises PSR; the device_id is provably not a
unique panel key (`65 44 15 67 62 61` is shared by the 15,1 and the 16,1, which
differ in DPCD revision, link rate and `0x071`); OUI-only matching with
`DEVICE_ID_ANY` is the idiomatic form in this table, including the Apple entry
being removed; and five machines across two display generations, two link
configurations and both values of `DP_PSR_NO_TRAIN_ON_EXIT` have now run it with
no failure and no sink error. `i915.enable_psr=0` remains the documented off
switch.


### Rebased onto drm-tip and compile checked (2026-08-27)

Shallow clone of `https://gitlab.freedesktop.org/drm/tip.git`, branch `drm-tip`,
at `108b145a7` (7.2.0 development, integration manifest of the same day). The
7.1.9-generated patches do not `git am` onto it, so the changes were re-applied
by hand. Two differences worth remembering:

* `enum intel_quirk_id` has gained `QUIRK_DISABLE_PSR2` since 7.1.9, so the new
  `QUIRK_APPLE_PSR_HANDSHAKE` goes after that one, not after
  `QUIRK_DISABLE_EDP_PANEL_REPLAY`.
* `intel_dpcd_quirks[]` has gained two Dell XPS entries, so the Apple entry moves
  to the end of a longer table. `psr_global_enabled()`, `_psr_enable_sink()`,
  `_psr_init_dpcd()`, `intel_init_dpcd_quirks()` and the `dpcd_quirk_list[]`
  Apple entry are otherwise byte identical to 7.1.9 - the failure was pure line
  drift, not a semantic change.

Resulting diffstat is identical to the 7.1.9 series: 5 files, 51 insertions, 16
deletions. `make drivers/gpu/drm/i915/display/intel_psr.o intel_quirks.o
drivers/gpu/drm/display/drm_dp_helper.o` against the Fedora config builds all
three objects with no warnings.

`checkpatch.pl --strict` is clean on patches 1 and 3. Patch 2 reports "Unknown
commit id '1035f4a65f58'", which is an artifact of `--depth=1`: the 2018 commit
is not in the shallow history. Verified independently against the GitHub mirror
that both the hash and the subject "drm/i915: Disable PSR in Apple panels" are
correct, and that the quoted sentence is verbatim from that commit message.

Series lives in `upstream-psr-v1/` in the repo root (untracked), together with
`send-email-command.txt` holding the recipient list from `get_maintainer.pl`.
Posted as plain `[PATCH]`, not v2 - the earlier v1/v2 were internal iterations
that never reached a mailing list.

### The OUI-wide decision was wrong, reversed after review (2026-08-27)

Review feedback from the project owner overturned the reasoning recorded above.
Recording the mistake, because the flawed argument was persuasive enough to make
it into a cover letter.

**The device_id argument was a non sequitur.** "The device_id is not a unique
key, therefore a whitelist is impossible" does not follow. Two *working* panels
sharing an ID is harmless for a positive list - both are supposed to match. The
shared ID would only be a problem if an untested *incompatible* panel used it.
Uniqueness of the key was never the question; coverage of the key was.

**The safety net argument addressed the wrong failure mode.**
`intel_psr_short_pulse()` catches a sink that reports an error. It does not
catch a sink that quietly stays inactive without raising one, and more
importantly it cannot catch anything at all in the case that actually matters:
`0x321`, `0x4d1`, `0x4d2` and `0x4d4` are vendor defined, so on a TCON
generation that maps them to something else the damage is done by the write
itself, before any status can be read back. No runtime safeguard covers that.

**`i915.enable_psr=0` is a recovery path, not a default.** Upstream expects the
default to be safe without a module parameter.

**"The OUI is Apple's own" proves the vendor, not the behaviour.** True and
irrelevant to whether the sequence is correct on an untested panel.

The earlier rejection of DMI does not apply to the shape that was proposed.
That rejection was aimed at `intel_dmi_quirks[]`, which sets display-level
quirks and carries the comment about systems without meaningful PCI subsystem
IDs. Calling `dmi_first_match()` *inside* the DPCD quirk hook is a different
construction: the hook only runs after the sink OUI matched, and it sets a
per-`intel_dp` quirk. Both objections evaporate. `<linux/dmi.h>` is already
included in `intel_quirks.c`, and the existing DMI tables there are plain
`const`, not `__initconst`, so a runtime match is safe.

### Final shape: two patches, i915 only

1. `drm/i915/psr: add Apple eDP TCON PSR handshake quirk` - the handshake, the
   VBT override, and `apple_psr_handshake_dmi_table[]` with the five tested
   models. The hook returns early with a `drm_dbg_kms` when DMI does not match.
2. `drm/i915/psr: allow PSR on Apple panels with the handshake quirk` - the
   `NO_PSR` check in `_psr_init_dpcd()` gains
   `&& !intel_has_dpcd_quirk(intel_dp, QUIRK_APPLE_PSR_HANDSHAKE)`.

An unnoticed benefit of the narrow shape: `drm_dp_helper.c` and
`drm_dp_helper.h` are no longer touched at all, so the series stops being
cross-tree. It goes to drm-intel alone, `get_maintainer.pl` no longer lists the
drm-misc maintainers, and the "who takes this" paragraph disappears from the
cover letter.

Diffstat 3 files, 94 insertions, 2 deletions. Both objects compile warning free
against the Fedora config; `checkpatch.pl --strict` clean apart from the known
shallow-clone "Unknown commit id" artifact.

The full T2 model list lives in
`security/integrity/platform_certs/load_uefi.c` - eleven notebooks plus five
desktops. The six untested notebook entries are parked in
`upstream-psr-v1/more-models.txt` rather than in the series.

### Measured: DMI list vs T2 PCI ID as the filter (2026-08-27)

The project owner proposed replacing the five-entry DMI table with a check for
the Apple T2 on the PCI bus, on the grounds that kernel patches should stay
small. Both variants were built and measured rather than argued about.

| | source lines added | .rodata | hook text |
|---|---|---|---|
| DMI list, 5 models | 94 | 2064 B (`apple_psr_handshake_dmi_table`) | 91 B |
| T2 PCI 106b:1801 | 73 | none | 111 B, helper inlined |

So the T2 variant really is 21 lines and about 2 KB smaller. For scale, i915.ko
is roughly 8 MB uncompressed, and the table is `.rodata` read once per connector
init, not code in any hot path. The size argument is real but small; the actual
difference between the two is scope, five tested models against eleven possible
ones.

The T2 device is `106b:1801` ("T2 Bridge Controller"), sitting next to the ANS2
NVMe controller, the Secure Enclave and the audio device on the same function
group. `PCI_VENDOR_ID_APPLE` is already in `pci_ids.h`; `0x1801` is not.

**The desktop question is moot either way, and for a stronger reason than the
machine wiring.** `intel_psr_init()` sets `intel_dp->psr.source_support` only
inside `if (HAS_PSR(display) && intel_dp_is_edp(intel_dp))`, so PSR, and with it
`_psr_enable_sink()` and the vendor writes, can only ever run on an internal eDP
panel. A Mac mini driving an external monitor, an iMac whose panel hangs off the
dGPU, a MacPro with no iGPU at all: none of them can reach this code, whatever
the filter says.

Kept the DMI list for submission. The size argument does not survive the
measurement, the scope question was already decided one round earlier, and DMI
is the idiom reviewers expect for "this machine model" (see the sixteen-entry
Apple table in `security/integrity/platform_certs/load_uefi.c`). Following the
owner's own fallback advice, the trade-off is now stated as an open question in
the cover letter instead of being settled unilaterally. The T2 variant is parked
on branch `t2-pci-variant` in `/home/alex/psr-upstream/tip`, one commit on top
of `psr-apple`.

### Switched to the T2 PCI filter (2026-08-28)

Project owner's call: enable the quirk on all T2 Macs rather than on the five
tested models, and drop the open question from the cover letter. Reverting is
cheap if the maintainers want it narrower.

`quirk_apple_psr_handshake()` now gates on `intel_has_apple_t2()`, a
`pci_get_device(PCI_VENDOR_ID_APPLE, 0x1801, NULL)` lookup with the reference
released immediately, testing only for presence. `0x1801` is defined locally as
`PCI_DEVICE_ID_APPLE_T2_BRIDGE`; `pci_ids.h` has the vendor ID but not this
device.

The `drm_dbg_kms` on the non-matching path was reworded from "not tested on this
machine" to "needs a T2 Mac, not applying" - the old text belonged to the DMI
list and would have misled anyone reading it on a non-T2 Mac.

Final series: 2 patches, i915 only, 3 files, 73 insertions, 2 deletions. Both
objects compile warning free, `checkpatch --strict` clean apart from the known
shallow-clone artifact. The DMI variant is gone from the working tree but is
recoverable from this document.

Cover letter now states the scope plainly, including that the remaining T2
notebooks are covered without having been tested, and the point that the T2
desktops cannot reach the code at all because `source_support` is only set for
`intel_dp_is_edp()`.

### Texts cut down before submission (2026-08-28)

The cover letter body had grown to 80 lines against a 73 line patch. Trimmed to
36. What came out was the "Scope" section, three paragraphs defending the
matching choice against objections nobody had raised, plus the sentence
justifying why the panel device ID is not used as a match key. Both commit
messages lost their justification paragraphs for the same reason.

The rule the project owner set: as short as possible, as long as necessary,
plain English, no defences and no answers to questions that have not been
asked. Worth keeping for the reply to review comments as well.

What stayed, because a maintainer needs it: the 2018 commit and its quote, the
traced register sequence, what each patch does and that patch 1 is inert alone,
the test base, the power number, and the fact that the untested T2 notebooks
match too. Plus one closing question about matching granularity, which is what
the project owner wanted a first submission to lead with.

### Collapsed to a single patch, and its review (2026-08-29)

The two patch series became one patch, `upstream-psr-v2/`. Splitting it was
never right: patch 1 alone was inert, and a commit that changes no behaviour is
hard to justify on its own. One patch, three files, no cover letter needed.

Re-verified against drm-tip `108b145a7`: applies cleanly, both objects compile
without warnings, `checkpatch --strict` reports 0 errors and 0 checks. The one
warning is still the shallow-clone "Unknown commit id" artifact.

Two things that were open in v1 are now settled:

- `intel_quirks.c` uses `pci_get_device`/`pci_dev_put` while including only
  `<linux/dmi.h>`. It builds, so `<linux/pci.h>` does arrive transitively. No
  explicit include added.
- The `DP_DPCD_QUIRK_NO_PSR` exception is complete. Grepped the whole tree: the
  flag has exactly one consumer, the check in `_psr_init_dpcd()` that the patch
  touches. Nothing in xe or any other driver reads it.

Ordering is safe by construction: `intel_dp_is_edp()` reads
`dig_port->base.type`, which the encoder init sets long before the first DPCD
read, so the eDP guard in the hook is reliable.

Three points to have an answer ready for in review:

1. Why a `pci_get_device` scan instead of the table's own `subsystem_vendor`
   field? The iGPU here reports subsystem `106b:0211`, so the table could match
   Apple natively and the hook would shrink to two lines. The answer: an Apple
   subsystem vendor means "Mac with an Intel iGPU", not "T2 Mac". It would pull
   in the 2016/2017 T1 machines, which carry the same panel OUI and which
   nobody has tested. The PCI scan matches exactly the tested class.
2. The commit message notes that macOS delays the `0x4d4` trigger by five
   seconds, while the patch writes it immediately. Tested that way on five
   machines, boot and suspend/resume.
3. The PSR2 path is not covered. With `has_sel_update` the CRC skip does not
   apply, because it sits in the `else` branch, and
   `DP_PSR_IRQ_HPD_WITH_CRC_ERRORS` would be set. Unreachable on every tested
   panel, since they are PSR1 only.

Two style worries checked and dropped: mid-file `#define`s are already normal in
`intel_psr.c` (`PSR_IDLE_TIMEOUT_MS`), and trailer order with `Tested-by` before
`Signed-off-by` is common enough that nobody blocks on it. The only real style
nit left is `PCI_DEVICE_ID_APPLE_T2_BRIDGE`, a prefix conventionally reserved
for `include/linux/pci_ids.h`; plenty of drivers define it locally anyway, and
it is a one-liner if it comes up.
