# How to configure GPUs

If a Mac has a dGPU, it will use it for boot and it will also use it as primary
display adapter by default. An iMac is no exception in that aspect, but it is
not able to switch between internal and dedicated GPU because the display lines
from iGPU to display are missing. So on iMacs, the iGPU is only used for offloading.
Thus, if you are an iMac user, this guide is not for you.
Same for Mac Pro users, since Mac Pros have no iGPU.
This guide is only for Macbook Pro users.

## Choose the boot GPU

KaiT2en installs **T2 GPU Control** on MacBook Pro models that have both Intel
and AMD graphics. Open it from the application menu to choose the primary GPU
for the next boot.

When the integrated GPU is selected, the app can also power off the discrete
GPU after boot. This saves a tremendous ammount of (battery) power.
The dGPU cannot accelerate applications while it is powered off.
Before suspend, the app's service powers the dGPU back on so its existing
AMDGPU binding can pass through suspend and resume. It restores the powered-off
state afterwards without unloading the driver or rebuilding vgaswitcheroo.

The AMDGPU power-saving option applies the driver's `POWER_SAVING` profile at
boot and restores it after resume. GPU discovery is dynamic, so it does not
depend on whether AMDGPU appears as `card1`, `card2`, or another DRM device.

**Apply Changes** stores the selected boot configuration. **Reboot** remains a
separate action so an accidental click does not immediately restart the system.
The helper refuses to power off the dGPU unless the integrated GPU is active.

## MacBook Pro 15,1 A1990 dGPU suspend issues

The AMDGPU driver cannot reliably resume the Radeon GPU in
the MacBookPro15,1. Suspend can therefore fail or resume to a black screen,
regardless of the configuration selected in T2 GPU Control.

Our upstream fix has been accepted and will be released with Linux 7.3.
Until then you can patch AMDGPU yourself using the patch in the patches
folder.
