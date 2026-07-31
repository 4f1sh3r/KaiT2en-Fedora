# T2 GPU Control

T2 GPU Control configures the primary GPU for the next boot on T2 MacBook Pro
models with Intel and AMD graphics. It can also arrange for the discrete GPU to
be powered off after an iGPU boot and apply AMDGPU's power-saving profile while
the discrete GPU remains powered.

The privileged helper accepts only fixed configuration operations. It refuses
to power off the discrete GPU unless vgaswitcheroo reports the integrated GPU as
active.

The main KaiT2en installer invokes the app-specific installer automatically on
supported hardware. It can also be run directly from the repository:

```bash
sudo ./apps/t2-dgpu-control/install.sh
```
