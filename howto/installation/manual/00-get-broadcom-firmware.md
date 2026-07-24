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

BT_BASE="/usr/share/firmware/bluetooth"
DEST="${1:-firmware}"

WIFI_FILES=$(log show --last boot --info \
  --predicate 'eventMessage contains "/usr/share/firmware/"' 2>/dev/null \
  | grep "Copying" \
  | grep -oE '"[^"]*"' | tr -d '"' | sort -u)

BT_CHIPSET=$(system_profiler SPBluetoothDataType 2>/dev/null \
  | grep "Chipset:" \
  | grep -oE 'BCM_[0-9A-Z]+' \
  | tr -d '_' \
  | head -n 1)
BT_FILES=""
if [[ -n "$BT_CHIPSET" && -d "$BT_BASE" ]]; then
  BT_FILES=$(find "$BT_BASE" -maxdepth 1 -type f \
    \( -iname "${BT_CHIPSET}*.bin" -o -iname "${BT_CHIPSET}*.ptb" \) \
    | sort)
fi

echo "[Wi-Fi]"
echo "$WIFI_FILES"
echo ""
echo "[Bluetooth PCIe firmware]"
if [[ -n "$BT_FILES" ]]; then
  echo "$BT_FILES"
else
  echo "No PCIe Bluetooth firmware found for ${BT_CHIPSET:-the detected chip}."
fi
echo ""

read -p "Copy files to '$DEST'? [y/N] " ANS
if [[ "$ANS" == "y" || "$ANS" == "Y" ]]; then
  mkdir -p "$DEST"
  while IFS= read -r f; do
    cp "$f" "$DEST/" && echo "  copied: $(basename "$f")"
  done <<< "$WIFI_FILES"
  if [[ -n "$BT_FILES" ]]; then
    while IFS= read -r f; do
      cp "$f" "$DEST/" && echo "  copied: $(basename "$f")"
    done <<< "$BT_FILES"
  fi
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

[Bluetooth PCIe firmware]
No PCIe Bluetooth firmware found for BCM4364B0.

Copy files to 'firmware'? [y/N] y
  copied: ekans.trx
  copied: kauai.clmb
  copied: kauai.txcb
  copied: P-kauai_M-HRPN_V-u__m-7.5.txt

Done.
```

On Macs with PCIe Bluetooth, the script also copies Apple's `.bin` and `.ptb`
Bluetooth firmware files. These files are only preserved at this stage. T2 Macs
use more than one Bluetooth architecture, and the Apple filenames must be
matched to the chip, board and antenna vendor before Linux can load them. Do not
rename or install them as a generic `BCM.hcd`. See the Bluetooth notes in the
Linux installation step.

Next: [Prepare macOS and the Fedora installer](01-prepare-macos-and-fedora-usb.md)
