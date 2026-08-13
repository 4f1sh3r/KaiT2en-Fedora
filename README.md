<p align="center">
  <img src="assets/kait2en-fedora.jpeg" alt="KAIT2EN logo" width="720">
</p>

# KAIT2EN Fedora

KAIT2EN brings cutting edge T2 Mac support to stock Fedora using DKMS modules.
You will receive kernel updates directly from Fedora and the latest T2 modules from us.

[Docs](https://kait2en.org/documentation.html) |
[Blog](https://kait2en.org/blog.html) |
[Community](#community) |
[Contributing](#contributing)

## Install

Follow the [installation guide](https://kait2en.org/documentation.html#installation).

## Community

Join the KAIT2EN community on [Discord](https://discord.gg/AGfjRk4ydj) or on
[Matrix](https://matrix.to/#/%23kait2en:matrix.org).

## Website

Everything behind [kait2en.org](https://kait2en.org) lives in `website/`:
documentation in `website/docs/`, blog posts in `website/blog/`, and
`website/site.yml` decides what appears where. CI publishes it on every push to
`main`.

```bash
pip install -r website/requirements.txt
python website/build.py --serve
```

## Contributing

Contributions are welcome, especially when they move KAIT2EN fixes closer to
clean upstream Linux support.

Please keep changes and PR descriptions focused. You may use AI for debugging,
but we will notice slop and refuse to review or merge obvious slop. We are not
interested in workarounds. There is a distinct difference between making broken
things work and fixing things.

## License

KAIT2EN-owned scripts, howto documents, project text and helper code are MIT
licensed.

Kernel modules, apps and third-party tools may include code with different
origins. Those components keep their own licenses in their directories.
