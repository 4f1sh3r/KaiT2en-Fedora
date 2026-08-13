---
title: The resume timer that lied to us
date: 2026-07-24
author: KAIT2EN
summary: >-
  Sample post. A long-standing resume delay on the T2 bridge turned out to be
  two bugs, one of them in the measurement itself.
tags: [t2bce, suspend]
---

*This is placeholder text for the new blog. Replace it with a real post.*

For months, waking a MacBook Pro from suspend took somewhere between thirty and
fifty-five seconds before the internal keyboard came back. The variance was the
interesting part: a bug with a fixed cost is usually one thing, and a bug that
takes anywhere in a twenty-five second window is usually several.

## Measuring the wrong thing

The first round of instrumentation logged `ktime_get()` around every stage of
the resume path. The numbers made no sense - stages that could not possibly take
long were reported as taking seconds, and the total never added up to the delay
the user actually felt.

The clock itself was the problem. Immediately after a real ACPI resume,
`ktime_get()` and the timestamps `printk` puts in front of every line are not
trustworthy on this hardware. Switching the instrumentation to
`ktime_get_boottime()` produced a timeline that finally matched a stopwatch.

> A profiler that shares the bug you are profiling will confirm whatever you
> already believe.

## What the timeline showed

With honest timestamps, the delay was not in our driver at all. It sat inside
`usbcore`, waiting on device enumeration that our resume path had already
requested. `function_graph` tracing with `tracing_thresh` set high enough to
drop the noise made it obvious:

```bash
echo function_graph > /sys/kernel/tracing/current_tracer
echo 20000 > /sys/kernel/tracing/tracing_thresh
```

Anything still in the trace after that threshold is, by definition, worth
looking at.

## Where it stands

The delay is fixed. One port still resets on resume - the one the internal
keyboard and trackpad hang off - and that reset is expected, quick, and
harmless. If you see it in `dmesg`, it is not the bug this post is about.
