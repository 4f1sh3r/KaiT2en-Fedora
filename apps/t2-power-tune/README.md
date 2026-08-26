## Reset

```sh
sudo systemctl disable --now kait2en-power-tune.service
sudo rm /etc/systemd/system/kait2en-power-tune.service
sudo rm -f /run/t2-power-tune/items.json
sudo systemctl daemon-reload
```

`--now` runs the unit's `ExecStop` commands and restores the saved values.

## Recovery when the system does not boot

Add this kernel parameter once in the bootloader:

```text
systemd.mask=kait2en-power-tune.service
```

After booting:

```sh
sudo systemctl disable kait2en-power-tune.service
sudo rm /etc/systemd/system/kait2en-power-tune.service
sudo systemctl daemon-reload
sudo reboot
```

`/run/t2-power-tune/items.json` is volatile and disappears on reboot.
