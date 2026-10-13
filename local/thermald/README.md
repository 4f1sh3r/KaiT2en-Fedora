# Rig thermald setup

Skin-driven pre-PROCHOT RAPL control for this MacBookPro16,2. The full
history and measurements are in `local/learnings/power/prochot-fan.md`.

- `thermal-conf.xml.in` — the actual config to edit: package zone 90/93 C
  plus the `t2_skin` zone (Ts0S/Ts1S, trip 44.8 C, `rapl_controller`,
  PPCC floor 12 W). `@Ts1S@`/`@Ts0S@` are expanded by the generator.
- `thermal-conf.xml` — the predecessor without the skin zone, kept for
  reference.
- `t2-thermald-genconf` — maps the SMC labels to the current hwmon paths
  (hwmonN is not stable across boots) and expands the template into
  `/run/thermald/thermal-conf.xml`.
- `kait2en-manual.conf` — drop-in that runs thermald against the generated
  config with `--ignore-default-control`.
- `kait2en-manual.conf.pre-skin` — the drop-in as it was before the skin
  zone was added (2026-09-04).

Enable on the machine: `sudo local/thermald/install.sh`
Disable (back to stock Fedora thermald): `sudo local/thermald/uninstall.sh`

Status: uninstalled on the machine since 2026-10-13. Upstream
`t2-zone-power` took over CPU power control; the skin floor stays out of
the way until the zone-power PROCHOT test has been judged.
