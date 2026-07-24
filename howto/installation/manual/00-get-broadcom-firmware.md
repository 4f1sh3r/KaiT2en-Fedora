# Get Broadcom firmware from macOS

[Installation introduction](../../introduction.md) | Next: [Prepare macOS and the Fedora installer](01-prepare-macos-and-fedora-usb.md)

*When you already have T2 Linux Fedora installed and you are using our revert
script, you can jump to [Install KAIT2EN modules and apps](03-install-kait2en-modules-and-apps.md).*

Boot into macOS.
Open the macOS Terminal in the root folder of this repository on your USB drive.
Then execute the code below. It takes some time to complete. This is normal:

```
bash <(cat << 'EOF'
#!/bin/bash

DEST="${1:-firmware}"

WIFI_FILES=$(log show --last boot --info \
  --predicate 'eventMessage contains "/usr/share/firmware/"' 2>/dev/null \
  | grep "Copying" \
  | grep -oE '"[^"]*"' | tr -d '"' | sort -u)

echo "[Wi-Fi]"
echo "$WIFI_FILES"
echo ""

read -p "Copy files to '$DEST'? [y/N] " ANS
if [[ "$ANS" == "y" || "$ANS" == "Y" ]]; then
  mkdir -p "$DEST"
  while IFS= read -r f; do
    cp "$f" "$DEST/" && echo "  copied: $(basename "$f")"
  done <<< "$WIFI_FILES"
  echo ""
  echo "Done."
fi
EOF
)
```

As an example, running the script on a MacBook Pro 15,1, this is the expected output:

```
[Wi-Fi]
/usr/share/firmware/wifi/C-4364__s-B2/ekans.trx
/usr/share/firmware/wifi/C-4364__s-B2/kauai.clmb
/usr/share/firmware/wifi/C-4364__s-B2/kauai.txcb
/usr/share/firmware/wifi/C-4364__s-B2/P-kauai_M-HRPN_V-u__m-7.5.txt

Copy files to 'firmware'? [y/N] y
  copied: ekans.trx
  copied: kauai.clmb
  copied: kauai.txcb
  copied: P-kauai_M-HRPN_V-u__m-7.5.txt

Done.
```

Bluetooth firmware is not copied by this procedure. T2 Macs use more than one
Bluetooth architecture, and the firmware exposed by macOS does not map
unambiguously to the firmware interfaces used by the Linux drivers. See the
Bluetooth notes in the Linux installation step before installing any Bluetooth
firmware manually.

Next: [Prepare macOS and the Fedora installer](01-prepare-macos-and-fedora-usb.md)
