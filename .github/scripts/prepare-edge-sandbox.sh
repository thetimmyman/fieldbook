#!/usr/bin/env bash
# Restore the packaged Edge sandbox on the disposable hosted runner; never disable it.
set -euo pipefail
helper=/opt/microsoft/msedge/msedge-sandbox
package=microsoft-edge-stable
[[ -f "$helper" && ! -L "$helper" ]]
[[ "$(/usr/bin/stat -c '%u:%g' "$helper")" == 0:0 ]]
mode="$(/usr/bin/stat -c '%a' "$helper")"
printf 'Edge sandbox helper: root:root mode %s\n' "$mode"
[[ "$mode" == 755 || "$mode" == 4755 ]]
[[ "$(/usr/bin/dpkg-query -S "$helper")" == "$package: $helper" ]]
manifest="$(/usr/bin/dpkg-query --control-path "$package" md5sums)"
expected="$(/usr/bin/awk '$2 == "opt/microsoft/msedge/msedge-sandbox" { print $1 }' "$manifest")"
[[ "$expected" =~ ^[0-9a-f]{32}$ ]]
printf '%s  %s\n' "$expected" "$helper" | /usr/bin/md5sum --check --status
if [[ "$mode" == 755 ]]; then
    /usr/bin/sudo /usr/bin/chmod 4755 "$helper"
fi
[[ "$(/usr/bin/stat -c '%u:%g:%a' "$helper")" == 0:0:4755 ]]
printf 'Edge sandbox helper verified: root:root mode 4755; packaged bytes match\n'
