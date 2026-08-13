# How to configure GPUs

If a Mac has a dGPU, it will use it for boot and it will also use it as primary
display adapter by default. An iMac is no exception in that aspect, but it is
not able to switch between internal and dedicated GPU because the display lines
from iGPU to display are missing. So on iMacs, the iGPU is only used for offloading.
Thus, if you are an iMac user, this guide is not for you.
Same for Mac Pro users, since Mac Pros have no iGPU.
This guide is only for Macbook Pro users.

## MacBookPro15,1: enable hybrid graphics

KaiT2en installs **T2 Hybrid GPU Control** on the MacBookPro15,1. Open it from
the application menu and enable **Hybrid graphics**.

Hybrid graphics makes the integrated GPU the display GPU. Applications can
still use the AMD GPU through PRIME offload. The kernel wakes it automatically
for accelerated work and returns it to D3cold when it becomes idle. This keeps
the dGPU available without paying its idle power cost.

The installer builds the required AMDGPU and HDA modules for the current Fedora
kernel. The app reports whether the required runtime-PM support is active.

The discrete-GPU boot option remains available as a recovery setting. Rebooting
is always a separate action so changing the stored boot GPU does not restart the
system unexpectedly.

Other Intel/AMD MacBook Pro models continue to use **T2 GPU Control**. Hybrid
runtime PM is not enabled on those models because their dGPU power-on path is
not yet reliable.
