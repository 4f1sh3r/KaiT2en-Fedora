#!/usr/bin/env bash

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/lib.sh"

status=0
python3 "$SCRIPT_DIR/acpi-autofix.py" || status=$?

if (( status >= 128 )); then
	exit "$status"
fi

if (( status != 0 )); then
	warn "ACPI autofix failed; continuing the KaiT2en installation"
fi

exit 0
