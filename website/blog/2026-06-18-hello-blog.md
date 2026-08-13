---
draft: true
title: A blog for the work between releases
date: 2026-06-18
author: KAIT2EN
summary: >-
  Sample post. Why kait2en.org now has a blog, and what will show up here
  between the feature board and the documentation.
tags: [meta]
---

*This is placeholder text for the new blog. Replace it with a real post.*

The feature board tells you what state a feature is in. The documentation tells
you how to use it once it works. Neither of them has room for the part in
between: why a fix looks the way it does, what we measured, and which of the
three obvious approaches turned out to be wrong.

## What goes here

Three kinds of posts, roughly:

- **Release notes.** What changed in the modules and in the installer, and
  whether you need to do anything about it.
- **Upstreaming reports.** A patch series went out, got review, and either
  landed or came back. Both outcomes are worth writing up.
- **Debugging write-ups.** The long ones. Suspend/resume on the T2 bridge has
  produced several of these already.

## What does not go here

Support questions. Those belong on
[Discord](https://discord.gg/AGfjRk4ydj) or in the
[issue tracker](https://github.com/kaiT2en/KaiT2en-Fedora/issues), where other
people can find the answer and where we can ask you for logs.

## How posts are written

A post is one Markdown file in `blog/` with a bit of YAML on top:

```yaml
---
title: A blog for the work between releases
date: 2026-06-18
author: KAIT2EN
summary: One or two sentences for the card on the blog index.
tags: [meta]
---
```

The filename date prefix is optional; the `date` field is what orders posts.
Set `draft: true` to keep a post out of the build. Everything below the front
matter is normal Markdown, and CI publishes it on the next push to `main`.

To see a draft before pushing it, build the site locally and open the address
it prints:

```bash
pip install -r website/requirements.txt
python website/build.py --serve
```
