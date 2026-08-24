# fastfetch logos

ASCII art generated from `assets/kaiT2en-logo-tr.png`. `kait2en.txt` uses quadrant
blocks plus shading characters for edges, the other two plain half blocks.

| file | size | content |
| --- | --- | --- |
| `kait2en.txt` | 56x24 | 改 mark above the KAIT2EN wordmark |
| `kait2en-small.txt` | 26x13 | 改 mark only |
| `kait2en-wide.txt` | 84x11 | full logo in one line, as in the original |

`$1` is the light logo color, `$2` the red of the T2.

Try one without installing anything:

```sh
fastfetch --logo-type file --logo assets/fastfetch/kait2en.txt \
          --logo-color-1 white --logo-color-2 red
```

Install:

```sh
sudo install -Dm644 assets/fastfetch/kait2en*.txt -t /usr/share/fastfetch/logos/
mkdir -p ~/.config/fastfetch
install -m644 assets/fastfetch/config.jsonc ~/.config/fastfetch/config.jsonc
```
