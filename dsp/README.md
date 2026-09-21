# Apple T2 Audio DSP

PipeWire/WirePlumber DSP graphs for the internal speakers of Apple T2 Macs
using the t2bce_audio driver and AppleT2 UCM configuration.
This is host-side processing, not a T2-exposed service or a kernel module.

The speaker graphs reproduce the model-specific processing that macOS applies
to the internal speakers: equalization and crossovers as `bq_raw` biquads,
the multiband compressor as lsp `mb_compressor_stereo`, the limiters as lsp
`limiter_stereo`, crosstalk cancellation as a side channel FIR
(`xtc-side-*.wav`) and virtual bass as bankstown. The graphs are generated;
edit them only through the KaiT2en maintainers.

The independent `t2-dsp` RPM/Debian package and `scripts/fedora/install-dsp.sh`
install all profiles to `/usr/share/t2-dsp/profiles/<profile>/`.
udev selects a short ALSA card ID from the DMI model at runtime, for example
`t2-15_1`. WirePlumber matches that ID and the specific UCM Speaker/Mic node.
There is no install-time PCI detection. Headphones and headset microphones
are excluded. Unsupported models are not renamed and receive no DSP graph.

Speaker graphs target the UCM split-PCM hardware node. Its name is preserved
so existing UCM loopbacks remain valid. Only the internal microphone node is
given a stable model-specific name. Raw devices remain available.

`make build` generates portable graphs and static rules without touching the
host. Filter coefficients and FIR data remain unchanged. `make test` checks
model coverage, routing, assets, staging and migration failure handling.
`make install DESTDIR=/path/to/staging PREFIX=/usr` stages the package without
activating anything. See [packaging/README.md](packaging/README.md) for builds,
dependencies, migration, activation and license details.

Layout: `profiles/` owns DSP data, `config/` the model mapping,
`integration/` runtime configuration and migration, `tools/` the build-only
generator, `tests/` validation and `packaging/` RPM/Debian recipes.

| Product | Profile |
|---|---|
| MacBookAir8,1 | `8_1` |
| MacBookAir8,2 | `8_2` |
| MacBookAir9,1 | `9_1` |
| MacBookPro15,1 | `15_1` |
| MacBookPro15,2 | `15_2` |
| MacBookPro15,3 | `15_3` |
| MacBookPro15,4 | `15_4` |
| MacBookPro16,1 | `16_1` |
| MacBookPro16,2 | `16_2` |
| MacBookPro16,3 | `16_3` |
| MacBookPro16,4 | `16_4` |
| iMac20,1 | `imac20_1` |
| iMacPro1,1 | `imacpro1_1` |


Required Fedora packages are declared by the RPM recipe and installed by
`install-dsp.sh`, not by the common dependency installer:

- `pipewire`
- `pipewire-pulseaudio`
- `wireplumber`
- `pipewire-module-filter-chain-lv2`
- `lv2-bankstown`
- `lsp-plugins-lv2`

Debian/Ubuntu use `pipewire-pulse`, `libspa-0.2-modules` and `bankstown-lv2`
for the corresponding PulseAudio compatibility, LV2 support and bass plugin.
udev is also required. There is no dependency on the other T2 services.
Reboot after installation. Neither package scripts nor the DSP source
installer restart user audio or rename a live card.

## Copyright

Copyright (C) 2026 André Eikmeyer, KAIT2EN. GPL-3.0-or-later with the
attribution term of `LICENSING.md`.

## Thanks

sharpenedblade contributed the udev/WirePlumber runtime-selection approach in
merged PR #63. This package adapts it to short ALSA IDs, current model coverage
and the UCM split-PCM layout.

lemmyg's [t2-apple-audio-dsp](https://github.com/lemmyg/t2-apple-audio-dsp)
did the pioneering work: the first PipeWire DSP graphs for T2 Macs, and the
template this module started from.
