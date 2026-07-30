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

On the MacBook 15,1 the SMU will die on suspend and resume with a black screen.
We have submitted a patch to upstream that solves this issue.

Until the patch is merged: if you want working suspend, you will need to configure
iGPU as primary and dGPU turned off as described above.
Also KAIT2EN will automatically install a script that will `modprobe -r amdgpu`
on suspend when it finds a 15,1.
