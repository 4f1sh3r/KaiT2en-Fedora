# Kait2en changes

This copy is based on `jessesung/hex2hcd`, commit
`6d30d778f548200beb4c3ae02176ac6952f3a7ea`.

Kait2en changes:

- accept Apple `.hex` files with CRLF line endings
- ignore a leading `$` line used by Apple firmware files
- derive the output filename from `.hex` to `.hcd` when no output path is given
- remove per-line debug output
- write the final Launch RAM command from the Intel HEX EOF address and the
  current extended linear address instead of always using `0xffffffff`

For `BCM4364B0-MiniDriver-uart.hex`, the final command becomes:

```text
4e fc 04 00 03 10 00
```

That is launch address `0x00100300`.
