# T2BCE Troubleshooting Learnings

Stand: 2026-07-01

Diese Datei sammelt die wichtigsten Erkenntnisse aus der bisherigen
Suspend/Resume-Fehlersuche an `t2bce` und `t2bdrm`. Sie ist bewusst als
Arbeitsnotiz geschrieben: Was wurde beobachtet, was wurde getestet, was ist
wahrscheinlich, und welche Wege haben sich nicht bewaehrt.

## Kontext

- Das Repo soll eine Vanilla-Fedora-Installation auf T2-Macs um T2-Support
  erweitern und ist als Alternative zum t2linux-Setup gedacht.
- `modules/t2bce` basiert funktional auf `apple-bce` und stellt unter anderem
  die internen HID-/USB-Geraete des T2 ueber einen virtuellen HCD bereit.
- `t2bdrm` haengt an der Touch-Bar-Seite und ist stark von einem gesunden BCE /
  USB-Zustand nach Resume ab.
- Ziel ist nicht ein Workaround, der zufaellig funktioniert, sondern ein
  belastbarer Fix fuer Suspend/Resume.

## Reproduzierbare Symptome

- Nach Suspend/Resume reagieren interne Tastatur und Trackpad je nach Testlauf
  kurz, dann gar nicht mehr, oder es kommt zu wiederholten/stotternden Events.
- Resume dauert teilweise sehr lange und fuehrt zu Kernel-Warnings aus dem
  Suspend-Testpfad, typischerweise weil Device-Resume deutlich ueber dem
  erwarteten Zeitbudget liegt.
- Typische Log-Signaturen:

```text
EP0 status=3 observed
EP0 status=3 deferred
command timeout cmd=16
usb 5-5: device descriptor read/64, error -110
usb 5-5: device firmware changed
usb 5-5: USB disconnect
t2bdrm ... Failed to send message (-110)
PM: resume devices took <lange Zeit>
```

- Ohne `react-drm` oder `tiny-dfr` kann die Touch Bar dunkel bleiben, aber das
  ist nicht automatisch "expected": Im HID-only/native Modus kann
  `t2touchbar_kbd` selbst den sichtbaren Special-Keys-Modus setzen. Wenn dieser
  Modus nicht sichtbar wird, ist das ein nuetzliches BCE/EP0-Symptom.

## Wichtige externe Hinweise

- Der T2 meldet offenbar mehr Information als zuerst genutzt wurde: nicht nur
  Portzustaende, sondern indirekt auch Endpoint-Bereitschaft.
- Die relevante Port-/Endpoint-Signatur ist `0x275`. Interpretation aus den
  Developer-Hinweisen: Der Port sieht schon brauchbar aus, der Endpoint ist aber
  noch nicht bereit. Wenn Linux dann einen Control-URB auf EP0 losschickt, lehnt
  der T2 den Transfer ab. Danach wird der URB stale und der Linux-USB-Stack
  laeuft in Reset/Re-enumeration.
- Trackpad, Keyboard, ForceTouch und Caps Lock sind im Sleep nicht einfach aus.
  Der T2 muss Teile davon weiter am Leben halten. Das macht einfache
  "alles pausieren und spaeter neu starten"-Annahmen gefaehrlich.

## Was bisher sicher gelernt wurde

- Das Problem sitzt primaer im `t2bce`-/VHCI-/Queue-/Resume-Pfad, nicht in
  `react-drm`.
- `t2bdrm` ist ein guter Trigger bzw. ein sichtbares Opfer des kaputten Zustands,
  aber nicht die eigentliche Root Cause.
- `modprobe -r t2bdrm` kann Suspend/Resume augenscheinlich stabilisieren. Das
  bedeutet: `t2bdrm` probed nach einer Re-enumeration zu frueh bzw. trifft auf
  einen noch instabilen BCE-Zustand. Es beweist aber nicht, dass `t2bdrm` selbst
  den initialen BCE-Fehler verursacht.
- Eine Re-enumeration kann das System in einen brauchbaren Zustand bringen, ist
  aber kein sauberer Fix. Sie erzeugt Nebeneffekte wie `device firmware changed`
  und t2bdrm-Timeouts.
- Das Entfernen von `t2bce` aus dem initramfs bzw. Timing-Aenderungen koennen das
  Verhalten beeinflussen, loesen aber das Protokollproblem nicht zuverlaessig.
- Ein Vergleich mit Kernel-Versionen 7.0.12 und 7.0.13 zeigte keine klare
  Regression nur durch diese Kernel-Minor-Version.
- Ein kompletter `t2bce`-Unload/Rebind im laufenden System kann das native
  Touch-Bar-Interface sichtbar machen. Das ist ein starker Hinweis auf ein
  Timing-/Bereitschaftsproblem beim fruehen Bootpfad, nicht auf fehlende
  Funktionalitaet der Touch-Bar-Treiber.
- Das sichtbare native Touch-Bar-Interface kommt im Config-1/HID-only-Pfad sehr
  wahrscheinlich von `t2touchbar_kbd`: Der Treiber setzt beim Probe per HID
  Output Report den Modus `APPLETB_KBD_MODE_SPCL` (`mode=2`). Wenn dieser
  Control-Transfer in einen EP0-status=3-Zustand laeuft, bleibt die Anzeige
  dunkel, obwohl HID/Input teilweise registriert ist.

## Aktuelle technische Interpretation

- Der kritische Bereich ist EP0 / Control-Transfer nach Resume.
- Nach Resume meldet der Port haeufig schon "connected/enabled/high-speed", aber
  der Endpoint ist noch nicht wirklich bereit.
- Der bisherige Resume-Code kann in eine Lage geraten, in der:
  - ein EP0-URB pending bleibt,
  - keine verwertbaren Events mehr kommen,
  - usbcore Descriptor Reads versucht,
  - diese Reads mit `-110` timeouten,
  - usbcore danach Port-Reset / Re-enumeration startet.
- Interner Port-Reset kann den Portstatus verbessern, belebt aber EP0 nicht
  automatisch wieder. Das war eine der wichtigsten Erkenntnisse der letzten
  Tests.
- Das gleiche `t2bce`/`t2touchbar_kbd`-Setup kann nach spaeterem Rebind
  funktionieren, waehrend es beim Boot scheitert. Der Unterschied ist im Log
  sichtbar: Beim fehlerhaften Bootpfad tritt `EP0 status=3` auf Port 6
  (Touch Bar Display) auf; nach dem spaeteren Rebind kommt die Touch-Bar-
  Enumeration ohne neue `EP0 status=3`-Zeilen durch.

## Patch-/Experiment-Historie

### Sinnvolle bzw. weiterhin relevante Richtungen

- Resume-Gating und zusaetzliches Logging um Portstatus, EP0 status=3 und Queue
  State waren sehr hilfreich.
- Port-Statusaenderungen, die nur durch Resume-Recovery entstehen, muessen
  vorsichtig behandelt werden. Nicht jede Firmware-Portaenderung sollte sofort
  an usbcore gemeldet werden.
- Das Logging von Queue-Zustand (`urbs`, `events`, `active`, `paused_by`,
  `stalled`, `tq_mask`) ist entscheidend, weil reine Portbits nicht ausreichen.
- Die interne Port-Reset-Recovery bleibt als Diagnose- und moeglicher
  Bestandteil eines spaeteren Fixes interessant: Sie kann Portbits reparieren,
  aber sie ist alleine nicht genug.

### Ansaetze, die sich nicht bewaehrt haben

- `t2bdrm` komplett als Root Cause zu behandeln: widerlegt durch Logs ohne
  `react-drm` und durch die BCE-Signaturen.
- `t2touchbar_cfg`/Config 2 allein als Erklaerung fuer das dunkle native
  Interface zu behandeln: zu einfach. Config 2 kann die native UI verdraengen,
  aber ein sauberer Config-1-Boot blieb ebenfalls dunkel, solange beim Boot ein
  EP0-status=3 auf Port 6 auftrat.
- Nur auf Port-Ready zu warten: nicht ausreichend, weil Endpoint-Bereitschaft
  separat betrachtet werden muss.
- Den nach interner Recovery folgenden usbcore-Port-Reset als No-op zu
  konsumieren:
  - Der No-op-Pfad wurde im Test tatsaechlich getroffen.
  - Danach blieben aber Descriptor Reads auf EP0 mit `-110` haengen.
  - Resume wurde schlechter, in einem Test ca. 33 Sekunden.
  - Danach kam trotzdem `device firmware changed` und Re-enumeration.
  - Schlussfolgerung: Der No-op kaschiert nur kurz den Reset, repariert aber den
    stale EP0-Zustand nicht.
- Externe Port-Changes nur ueber `hub_status_data()`/`GetPortStatus` zu
  verstecken, bis `port_change_waiting` leer ist:
  - Getestet mit `t2bce`-`srcversion` `2CD97350869FF94CBC03D2A`, Commit
    `302f89b`.
  - Der Boot zeigte zwar wie erwartet `visible_pending=0` und viele Zeilen der
    Form `hub GetPortStatus deferring external C_CONNECTION ... pending=7f
    waiting=7f`, aber usbcore enumerierte trotzdem weiter (`SetPortFeature
    RESET`, `new high-speed USB device`, `bce_vhci_enable_device`).
  - Damit ist belegt: Das Maskieren der Change-Bits reicht nicht aus, um
    usbcore vom fruehen Zugriff abzuhalten. Der Root-Hub-/Hub-State-Pfad fragt
    Portstatus trotzdem aktiv ab und reagiert auf den verbundenen/enabled
    Status.
  - Schlimmer: `port_change_waiting` blieb im Test ueber die Session auf `7f`
    haengen. Direkt vor dem Suspend gab es weiterhin
    `GetPortStatus deferring external C_CONNECTION ... waiting=7f`.
  - Der anschliessende Suspend endete bei `PM: suspend entry (deep)` ohne
    spaeteres Resume-Log; der Mac wachte nicht mehr auf und musste hart
    neugestartet werden.
  - Schlussfolgerung: Externe Connection-Changes nur im Root-Hub-Reporting zu
    maskieren ist kein sicherer Boot-/Resume-Gate-Fix. Dieser Ansatz kann einen
    inkonsistenten Gate-Zustand konservieren und Resume schlechter machen.

## Besonders aussagekraeftiger Logbefund

Ein Testlauf mit aktivem No-op-Pfad zeigte:

```text
tq ep0 status3 deferred ... urbs=1 events=0
tq local resume dev=7 ep=00 ... urbs=1 events=0
hub SetPortFeature RESET port=5 no-op after internal recovery
usb 5-5: device descriptor read/64, error -110
usb 5-5: device firmware changed
```

Das ist der Kernbefund: Der Port kann nach interner Recovery wieder "ready"
aussehen, aber EP0 ist trotzdem nicht in einem nutzbaren Transferzustand.

Ein spaeterer Touch-Bar-Test lieferte einen zweiten Kernbefund. Nach dem
Blockieren von `t2touchbar_cfg` und `t2bdrm` bootete die Touch Bar korrekt in
Config 1:

```text
5-6 bConfigurationValue = 1
5-6:1.0 Driver = usbhid
t2touchbar_kbd ... Touch Bar Display
```

Trotzdem blieb das Interface im ersten Bootpfad dunkel. Im gleichen Boot waren
die einzigen `EP0 status=3`-Zeilen:

```text
EP0 status=3 observed dev=2 port=6
EP0 status=3 deferred dev=2 port=6
tq ep0 status3 deferred ... urbs=1 events=0
```

Nach einem kompletten Laufzeit-Rebind von `t2bce` wurde Bus 5 neu aufgebaut,
`t2touchbar_kbd` band erneut, der Modus stand auf `2`, und das native Interface
wurde sichtbar. In diesem Rebind-Fenster trat kein neuer `EP0 status=3` auf.

## Aktuelle Schlussfolgerung fuer den naechsten Fix

- Nicht versuchen, usbcore sichtbare Resets nur zu verstecken.
- Der naechste Fix sollte Boot- und Resume-Gating gemeinsam betrachten:
  usbcore bzw. HID-Treiber duerfen Control-Transfers auf EP0 erst ausloesen,
  wenn der jeweilige Port und EP0 wirklich bereit sind.
- Stattdessen EP0 / Endpoint-State gezielt nach Boot/Resume reparieren oder
  den ersten Control-Transfer so verzoegern, dass er nicht in den status=3-
  Zustand laeuft.
- Moegliche naechste Experimente:
  - EP0 nach internem Port-Recovery gezielt resetten oder reinitialisieren.
  - Deferred EP0-URBs sauber abbrechen oder retrybar machen, statt sie stale
    haengen zu lassen.
  - Logging direkt um die Frage erweitern: Wann kommt nach status=3 wieder ein
    Transfer-Request/Event fuer EP0, und was ist der Queue-Zustand davor/danach?
  - Bootpfad und Rebindpfad direkt vergleichen: Warum ist `t2touchbar_kbd`
    Mode-Set beim spaeteren Rebind erfolgreich, beim Boot aber nicht?
- Jede neue Aenderung sollte mit klarer Log-Signatur kommen und wieder entfernt
  werden, wenn sie sich nicht als nuetzlich erweist.

## Test- und Analyse-Checkliste

Nach jedem Testlauf pruefen:

- Laeuft wirklich das gepatchte Modul?
  - `modinfo t2bce | grep srcversion`
  - geladene Version in `/sys/module/t2bce/srcversion`
- Gab es `EP0 status=3 observed/deferred`?
- Sind `command timeout cmd=16` aufgetreten?
- Bleibt fuer EP0 `urbs=1 events=0` stehen?
- Kommen `device descriptor read/64, error -110`?
- Kommt `device firmware changed`?
- Beim Touch-Bar-Test: ist `5-6` in Config 1 oder Config 2?
- Beim Touch-Bar-Test: ist `t2touchbar_kbd` gebunden und steht sein `mode`-
  Attribut auf `2`?
- Beim Touch-Bar-Test: treten `EP0 status=3`-Zeilen auf Port 6 auf?
- Wie lange dauerte `PM: resume devices took ...`?
- Wird `t2bdrm` erfolgreich registriert oder endet es in `-110`?
- Gab es eine Kernel-Warning, und wenn ja, nur wegen langer Resume-Zeit oder
  wegen eines neuen Problems?

## Hygiene-Regel fuer weitere Arbeit

- Diagnostische Patches sind willkommen, aber sie sollen nicht dauerhaft im Code
  bleiben, wenn sie keinen klaren Nutzen zeigen.
- Ein Patch gilt erst als hilfreich, wenn Logs zeigen, dass er den erwarteten
  Zustand wirklich verbessert und keine neue, schlechtere Signatur einfuehrt.
- Lieber kleine, einzeln revertierbare Commits als grosse kombinierte Aenderungen.

## 2026-07-03: Resume mit sichtbarer Touch Bar, aber Fn-Grafik stale

Teststand: `codex/t2bce-resume-port-state-gating`, geladene
`t2bce`-Srcversion `525CFD6578D4064839B2B08`.

Beobachtungen:

- Suspend/Resume funktionierte, dauerte aber weiterhin lange.
- Die Touch Bar blieb nach Resume sichtbar. Das spricht dafuer, dass das
  Resume-Gating fuer Port 6/7 zumindest den haertesten Fehler verhindert.
- Port 6 kam im Log aus `0x275` per `PORT_RESUME` auf `0x215`.
  Das passt zu Deqrocks' Hinweis: `0x275` bedeutet nicht "Port tot", sondern
  "Endpoint noch nicht bereit"; ein sofortiger Reset/Re-enum ist dort falsch.
- Port 5, also Topcase/Keyboard/Trackpad, blieb dagegen auf `0x285` bzw.
  `0x40285` haengen. Wiederholtes `PORT_RESUME` reichte dort nicht.
- Danach startete usbcore einen Reset-Sturm ueber mehrere Ports. Fuer Ports,
  die bereits `0x215`/ready waren, ist das wahrscheinlich schaedlich, weil es
  funktionierende Devices unnoetig neu aufbaut.
- Beim Fn-Druecken nach Resume funktionierten die F-Keys logisch, aber die
  Touch-Bar-Grafik blieb im alten Layout. Im Kernel-Log dazu:
  `BUG: scheduling while atomic` mit Stack ueber
  `appletb_kbd_set_mode -> hid_hw_request -> usb_hcd_unlink_urb ->
  bce_vhci_urb_request_cancel -> bce_vhci_transfer_queue_do_pause`.

Folgerungen:

- `0x275` und `0x285` muessen getrennt behandelt werden. `0x275` darf nicht
  als sofortiger Reset-Fall behandelt werden. `0x285` auf Port 5 ist der
  schwierigere Fall; der T2 scheint dort nach Resume manchmal nur mit einem
  echten Recovery/Reset wieder voll weiterzukommen.
- Der Fn-Grafikfehler ist nicht nur ein Touch-Bar-UI-Problem. Der aktuelle
  `t2touchbar_kbd`-Pfad schickt den HID-Mode-Report synchron aus dem
  Input-Callback. Auf t2bce kann dieser Pfad schlafen/blockieren und ist
  deshalb nach Resume gefaehrlich.

Neue Test-Patches:

- `246fc74 t2bce: suppress resume reset storm`
  - Resume-Waiting wird zuerst auf bekannte Device-Ports begrenzt.
  - Disconnected-Port-Zustaende wie `0xa1` werden nicht mehr faelschlich als
    suspended behandelt.
  - Waehrend des Resume-Guards werden Hub-Port-Resets fuer nicht-stuck Ports
    unterdrueckt; Port 5 darf nach ausgeschoepften Resume-Retries weiterhin
    als expliziter Topcase-Recovery-Fall resetten.
  - Logs enthalten nun `guard_left_ms`, damit sichtbar wird, ob der Guard
    wirklich noch aktiv ist.
- `305238c t2touchbar: defer fn mode reports from input path`
  - Fn-getriggerte Touch-Bar-Mode-Reports werden in eine Workqueue verschoben.
  - Der logische `current_mode` wird sofort aktualisiert, damit die F-Key-
    Funktion nicht auf den asynchronen Report warten muss.
  - Ziel: keine `scheduling while atomic`-Warnings mehr beim Fn-Umschalten.

Folgeanalyse desselben Teststands:

- Der Workqueue-Patch wirkt gegen die Kernel-Warning: Beim Fn-Test tauchte keine
  neue `scheduling while atomic`-Warnung mehr auf.
- Der Grafikwechsel blieb trotzdem kaputt. Logs zeigten vor und nach Suspend:
  `touchbar mode queued from input path: mode=1`, direkt gefolgt von
  `usb_submit_urb(ctrl) failed: -1`.
- Damit ist klar: der Fn-Pfad feuert, aber der Control-URB fuer den HID-
  Output-Report kommt nicht sauber durch.
- Kurz nach der Enumeration von `usb 5-6` (`05ac:8302`, Touch Bar Display)
  trat bereits beim Boot ein EP0-Status `3` auf:
  `bce-vhci: [00] URB failed - expected behaviour when 3 but TODO: 3`.
- Der alte Code setzte bei diesem Status die EP0-Queue auf
  `active=false/stalled=true`. Danach kann die Touch Bar zwar das zuletzt
  angezeigte Bild weiter darstellen und logisch Eingaben liefern, neue
  Mode-Reports bleiben aber im Control-Pfad stecken oder werden von usbcore/
  usbhid abgewiesen.

Neuer Test-Patch:

- `26cd2bc t2bce: keep ep0 active after status3`
  - EP0-Status `3` wird weiterhin als fehlgeschlagener Control-URB an usbcore
    abgeschlossen, aber die EP0-Queue wird nicht mehr dauerhaft auf
    `stalled/inactive` gesetzt.
  - Der Port-Recovery-Pfad (`port_resume_requested` /
    `port_change_waiting`) wird weiter aktiviert.
  - Neue EP0-Logs zeigen, ob spaetere Control-URBs auf einer inaktiven,
    pausierten oder gestallten Queue landen:
    `EP0 enqueue ... active=... paused_by=... stalled=...`.

Folgeanalyse:

- Der `keep ep0 active`-Patch reichte nicht. Nach dem naechsten Test blieb die
  Touch-Bar-Grafik weiterhin stale.
- Entscheidend war das neue Log:
  `EP0 enqueue dev=2 port=6 active=0 paused_by=0 stalled=0 state=2`.
- `state=2` ist `BCE_VHCI_ENDPOINT_STALLED`. Damit war die lokale
  `stalled`-Boolean zwar geloescht, der Firmware-Endpunktzustand stand aber
  weiterhin auf STALLED. Ergebnis: keine Pause, kein lokaler Stall, aber auch
  kein aktiver Endpunkt. Das ist ein Zombie-Zustand.

Neuer Test-Patch:

- `d2555d9 t2bce: reset ep0 after status3 stalls`
  - EP0-Status `3` setzt EP0 bewusst auf stalled/inactive und startet immer
    den vorhandenen Endpoint-Reset-Worker.
  - Der Worker sendet `ENDPOINT_RESET` und danach `ENDPOINT_SET_STATE ACTIVE`.
  - Firmware-Events, die EP0 explizit auf STALLED setzen, werden nun geloggt
    und triggern ebenfalls den EP0-Reset-Worker.
  - Erwartete gute Signatur: nach `EP0 status=3 ... scheduling endpoint reset`
    sollten `tq reset start/command/done ... ep=00 ... state=0 active=1`
    folgen. Danach sollten Fn-Mode-Reports nicht mehr auf
    `active=0 state=2` laufen.

Folgeanalyse:

- Der EP0-Reset lief tatsaechlich durch und setzte EP0 kurzzeitig auf
  `active=1 paused_by=0 state=0`.
- Spaetere Fn-Mode-Reports trafen aber auf
  `active=0 paused_by=1 stalled=0 state=1`.
- `paused_by=1` ist `BCE_VHCI_PAUSE_INTERNAL_WQ`, `state=1` ist
  `BCE_VHCI_ENDPOINT_PAUSED`. Damit bleibt nicht mehr der STALLED-Zustand
  haengen, sondern ein interner Cancel-/Reset-Pausepfad kommt nicht sauber
  nach ACTIVE zurueck.

Neuer Test-Patch:

- `ea6da09 t2bce: recover stale ep0 internal pause`
  - EP0-Pause/Resume-Transitions werden fuer `pause_lock`-Pfade geloggt:
    `tq pause ... src=...`, `tq resume begin/done ...`,
    `tq resume set-active failed/returned non-active ...`.
  - Wenn ein neuer EP0-Control-URB auf
    `active=0 paused_by=1 stalled=0 state=1` trifft, versucht t2bce vor dem
    Linken des URB `resume(INTERNAL_WQ)`.
  - Erwartete gute Signatur:
    `EP0 enqueue recovering stale internal pause ...` gefolgt von
    `tq resume ... ret=0 ... active=1 paused_by=0 state=0`.
