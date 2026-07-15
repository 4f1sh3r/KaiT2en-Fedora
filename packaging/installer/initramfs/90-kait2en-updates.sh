#!/bin/sh

source_image=/kait2en-anaconda-updates.img
target_directory=/run/kait2en

if [ ! -f "$source_image" ]; then
    warn "KaiT2en: Anaconda updates image is missing"
    exit 1
fi

mkdir -p "$target_directory"
cp "$source_image" "$target_directory/updates.img"

firmware_source=/kait2en-wifi-firmware
if [ -d "$firmware_source" ]; then
    mkdir -p "$target_directory/apple-firmware"
    cp -r "$firmware_source"/. "$target_directory/apple-firmware/"
fi
