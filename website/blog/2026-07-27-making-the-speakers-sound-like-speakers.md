---
title: Making the speakers sound like speakers
date: 2026-07-27
author: Andre Eikmeyer
summary: >-
  Getting sound out of the speakers was only the first half. Making it usable
  without crackling, wrong routing or damaged speakers became its own project.
tags: [audio, dsp]
---

There is a big difference between "the audio device works" and "this sounds
like the MacBook I paid for".

Apple's internal speakers are not driven like ordinary laptop speakers. The
hardware expects processing and protection that Linux does not magically know
about. Raw output can sound tinny, route to the wrong place, crackle while the
volume changes, or become dangerous at higher levels.

So audio turned into a project inside the project.

## First make it reliable

Before touching sound quality, the transport had to stop falling over. The T2
audio device sits behind the BCE stack, together with several other internal
devices. We split that stack into clearer modules, fixed headphone timing and
worked through the kind of bugs that only appear when a jack is inserted at
exactly the wrong moment.

One of the more visible annoyances was the GNOME volume slider. It sent a
stream of redundant volume updates while being dragged, and a DSP sink made
every one of them audible as a small crackle. The eventual fix went to GNOME
Shell rather than becoming another local workaround.

That fix was
[merged into GNOME Shell](https://gitlab.gnome.org/GNOME/gnome-shell/-/merge_requests/4302),
so it also benefits DSP setups outside KAIT2EN.

## Then make it sound right

The KAIT2EN DSP audio profiles use measured filters and speaker protection. They
are model-specific because a filter measured for one enclosure is not a safe
guess for another. This is why support grows machine by machine and why we ask
for real measurements instead of copying a profile until the sound is no
longer obviously terrible.

The result is still downstream. It is tied closely to Apple hardware and is
not a neat generic kernel feature. But it also means the internal speakers can
finally be used to actually enjoy media, with a raw sink still available when
someone needs it.

We will need to change device discovery still and make this easier to package
for distro maintainers. Because that is the place where it should be maintained.
ALSA don't want it, wireplumber don't want it and also pipewire won't. So this
must be a distro thing I believe.

Thanks go out to @lemmyg for the base DSP implementationa and also to ASAHI Linux,
of which we could use some FIRs.
