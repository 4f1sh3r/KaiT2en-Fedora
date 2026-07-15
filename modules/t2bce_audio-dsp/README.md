# Apple T2 Audio DSP

PipeWire/WirePlumber DSP graphs and FIR files for Apple T2 audio.

The files in `firs/` are imported from `t2-apple-audio-dsp` and are installed
by `scripts/fedora/install-dsp.sh` when the current Mac model has a matching
profile.

Supported profiles:

- `MacBookPro16,1` -> `16_1`
- `MacBookPro16,4` -> `16_4`
- `MacBookAir9,1` -> `9_1`
- `MacBookPro15,1` -> `15_1` (experimental 4-channel profile derived from
  the shared 16,x tweeter/woofer FIRs)

The installer copies the matching files to:

```text
/usr/share/kait2en/audio-dsp/<profile>/
```

It generates a WirePlumber configuration at:

```text
/etc/wireplumber/wireplumber.conf.d/51-kait2en-t2-dsp.conf
```

The graph target is rewritten at install time to match the detected Apple T2
audio PCI device and KaiT2en UCM sink/source names.

Required Fedora packages are installed by `install-dsp.sh`, not by the common
dependency installer:

- `pipewire`
- `pipewire-pulseaudio`
- `wireplumber`
- `pipewire-module-filter-chain-lv2`
- `lv2-bankstown`
- `lv2-triforce`
- `lsp-plugins-lv2`
- `lv2-swh-plugins`

License: see `LICENSE`.
