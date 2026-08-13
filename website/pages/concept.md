### This Is Not Easy to Explain

The Linux mainline kernel still lacks drivers for certain T2 hardware components. The thing with T2 drivers is that they require reverse engineering. Which is particularly difficult on Apple hardware because Apple stuff is "thought different". There are not many developers who still deal with T1/T2 hardware on Linux. Many moved on to Apple Silicon for understandable reasons.

The flow usually is, that someone writes a driver by reverse engineering. In the best case a community would test it and then it would be submitted to upstream maintainers to check and merge. Some T2 drivers indeed went that way and now live in the upstream kernel. Some are still missing. And these are the most complex drivers. Because of their complexity they are not easy to upstream. Also the devs who wrote them knew there were still issues with the code and it's architecure. Upstreaming a driver like that would create a lot of noise and work. So time went by... Devs moved on.

### Going Upstream = Moving Against The Flow

Traditional distros like CachyOS or T2 Linux solve this by shipping patched kernels. The patches are not created by the distro people. They use the code we talked earlier about and patch it into the kernel. The downside is you lose the standard distribution kernel, and updates are delayed while you wait for maintainers to rebuild each new release. Also when you file an issue on CachyOS or T2Linux GitHub, you are not talking to the actual developer behind the driver code.

Since we are currently the provider of many of these missing drivers, we know that getting updates into external distros takes time and can be difficult. Many third-party maintainers also naturally lack deep T2-specific context, making regressions more likely. Also informational flow can be difficult when breaking changes are introduced. Maintainers have to deal with incredibly complex workflows. The more you go upstream, the more complex it gets.

With KAIT2EN we jump over the middlemen. It is a unified platform. We take care that it works on your Mac. So we know your hardware, your commandline, your drivers, your systemd units, your udev rules etc... Users can test, talk to devs directly and we can submit to upstream. Third party maintainers can choose to package our code or just profit from upstreamed patches.

### Stream Me Up, Linus!

So KAIT2EN is specialized exclusively for T2 Macs. We deliver T2 drivers and dedicated T2 utilities. For you this is like cherry-picking: standard upstream kernels directly from Fedora, combined with immediate hardware fixes straight from us.

Because we rely on out-of-tree modules, we can test and iterate without full kernel recompilations. This streamlined architecture lets us roll out fixes and handle feature requests in minutes. Literally. All while working toward our goal, which is upstreaming every driver into the official Linux kernel, while dropping them downstream. This means, when we are done, T2 people can install Linux from official sources just like everyone else. And specialized distros or repos that aree scattered all around the interwebs are no longer needed to maintain the code.

### Yes, We Know There Is Apple Silicon...

But someone meeds to close the gap! We truly believe T2 MacBooks can make the perfect Linux laptops. Once everything is properly fixed, models like the MacBook Pro 15,1 or MacBook Air 9,1 run cool, offer great battery life, and cost very little. All while keeping Apple’s exceptional build quality, Retina displays, and Touch Bar.

So this is x86 architecture and we won't get anywhere near to what Apple Silicon/Asahi can do. But the message is not to buy into T2 Macs. It's about making them usable and act sustainable. If you already own a T2 Mac, you will appreciate. Because you know and we know that this era of devices was always kinda meh! Even at their time. But on Linux they are great. Even the "portable egg fryer" MacBook Air 9,1 is. 

And actually, before Apple began with their security chip shenanigans, Apple computers have always been great for Linux.

### Is The Grass Greener On The KAIT2EN Side?

Our grass is KAIT2EN red. There is a lot of discussions and arguing involved when you want to get things moving. It's the sound of grinding gears while trying to find solutions for everyone. We move fast, and our frequent update cycle might feel relentless and annoying. Staying informed means following announcements in our Discord community or checking GitHub for updates. We wouldn't recommend KAIT2EN to total Linux noobs. But we are surprised to see how fast people grow with new tasks.

Updating is entirely up to you, but KAIT2EN is built for active testing, not passive convenience. If you want to call us opinionated, then this is your chance. We share this project because we need real-world testers to validate our fixes on a base we know. Not updating will lead to a non-working Mac once outdated DKMS modules will stop compiling against an updated kernel that contains new symbols.

This is something you should keep in mind before jumping in.