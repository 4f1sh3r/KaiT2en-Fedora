### Now This Is not Easy to Explain

The Linux mainline kernel still lacks drivers for certain T2 hardware components. Traditional distributions like CachyOS or T2 Linux solve this by shipping custom-patched kernels. The downside is you lose the standard distribution kernel, and updates are delayed while you wait for maintainers to rebuild each new release.

Since we are currently the source of many of these missing drivers, getting updates into external distributions takes time. Many third-party maintainers also naturally lack deep T2-specific context, making regressions more likely. With KAIT2EN we jump over that. Users can test and we can submit to upstream directly. Third party maintainers can choose to package our code or just profit from upstreamed patches.

### Stream Me Up, Linus!

KAIT2EN is specialized exclusively for T2 Macs, it delivers T2 support to unmodified Fedora Linux using DKMS modules and dedicated T2 utilities. For you this is like cherry-picking: standard upstream kernels directly from Fedora, combined with immediate hardware fixes straight from us.

Because we rely on out-of-tree modules, we can test and iterate without full kernel recompilations. This streamlined architecture lets us roll out fixes and handle feature requests in minutes. Literally. All while working toward our goal, which is upstreaming every driver into the official Linux kernel.

### Yes, We Know There Is Apple Silicon

Our motivation is simple: we truly believe T2 MacBooks can make the perfect Linux laptops. Once everything is properly fixed, models like the MacBook Pro 15,1 or MacBook Air 9,1 run cool, offer great battery life, and cost very little. All while keeping Apple’s exceptional build quality, Retina displays, and Touch Bar. We even recently got hybrid graphics working on the 15,1 and built a custom Audio DSP setup. Because these features are difficult to package in traditional distributions, they are currently exclusive to KAIT2EN while we work on upstreaming them.
So this is x86 architecture and we won't get anywhere near to what Apple Silicon/Asahi can do. But the message is not to buy into T2 Macs. It's about making them usable and act sustainable. If you already own a T2 Mac, you will appreciate. Because you know and we know that this era of devices was always kinda meh! even at their time. But on Linux they are great. Even the "portable pan": MacBook Air 9,1.

### Is The Grass Greener On The KAIT2EN Side?

Our grass is KAIT2EN red. There is a lot of discussions and arguing involved. It's the sound of moving gears while trying to find solutions for everyone. We move fast, and our frequent update cycle might feel relentless. Staying informed means following announcements in our Discord community or checking GitHub for updates.

Updating is entirely up to you, but KAIT2EN is built for active testing, not passive convenience. We share this project because we need real-world testers to validate our fixes. Not updating will lead to a non-working Mac once outdated DKMS modules will stop compiling against a updated kernel.
This is something you should keep in mind before jumping in.