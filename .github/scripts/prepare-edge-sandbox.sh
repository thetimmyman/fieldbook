#!/usr/bin/env bash
# Restore the packaged Edge sandbox on a disposable hosted runner; never disable it.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
helper=/opt/microsoft/msedge/msedge-sandbox
package=microsoft-edge-stable
[[ -f "$helper" && ! -L "$helper" ]]
[[ "$(/usr/bin/stat -c '%u:%g' "$helper")" == 0:0 ]]
mode="$(/usr/bin/stat -c '%a' "$helper")"
printf 'Edge sandbox helper: root:root mode %s\n' "$mode"
[[ "$mode" == 755 || "$mode" == 777 || "$mode" == 4755 ]]
for directory in /opt /opt/microsoft /opt/microsoft/msedge; do
    [[ -d "$directory" && ! -L "$directory" ]]
    [[ "$(/usr/bin/stat -c '%u:%g' "$directory")" == 0:0 ]]
    directory_mode="$(/usr/bin/stat -c '%a' "$directory")"
    printf 'Edge ancestor: %s mode %s\n' "$directory" "$directory_mode"
    [[ "$directory_mode" == 755 || "$directory_mode" == 777 ]]
done
/usr/bin/dpkg-query -W -f='${Package} ${Version}\n' "$package"
[[ "$(/usr/bin/dpkg-query -S "$helper")" == "$package: $helper" ]]
manifest="$(/usr/bin/dpkg-query --control-path "$package" md5sums)"
[[ "$manifest" == /var/lib/dpkg/info/microsoft-edge-stable.md5sums ]]
for trusted in / /var /var/lib /var/lib/dpkg /var/lib/dpkg/info "$manifest"; do
    [[ ! -L "$trusted" ]]
    [[ "$(/usr/bin/stat -c '%u:%g' "$trusted")" == 0:0 ]]
    trusted_mode="$(/usr/bin/stat -c '%a' "$trusted")"
    [[ "$trusted_mode" =~ ^[0-7]{3,4}$ ]]
    (( (8#$trusted_mode & 0022) == 0 ))
done
[[ -f "$manifest" ]]
expected="$(/usr/bin/awk '$2 == "opt/microsoft/msedge/msedge-sandbox" { print $1 }' "$manifest")"
[[ "$expected" =~ ^[0-9a-f]{32}$ ]]
# Secure every replaceable ancestor before staging a new root-private inode.
for directory in /opt /opt/microsoft /opt/microsoft/msedge; do
    [[ -d "$directory" && ! -L "$directory" ]]
    [[ "$(/usr/bin/stat -c '%u:%g' "$directory")" == 0:0 ]]
    directory_mode="$(/usr/bin/stat -c '%a' "$directory")"
    [[ "$directory_mode" == 755 || "$directory_mode" == 777 ]]
    /usr/bin/sudo /usr/bin/chmod 0755 "$directory"
    [[ "$(/usr/bin/stat -c '%F:%u:%g:%a' "$directory")" == 'directory:0:0:755' ]]
done
[[ -f "$helper" && ! -L "$helper" ]]
[[ "$(/usr/bin/stat -c '%u:%g' "$helper")" == 0:0 ]]
mode="$(/usr/bin/stat -c '%a' "$helper")"
[[ "$mode" == 755 || "$mode" == 777 || "$mode" == 4755 ]]
root_scratch="$(/usr/bin/sudo /usr/bin/mktemp -d /opt/microsoft/msedge/.fieldbook-sandbox.XXXXXXXXXX)"
[[ "$root_scratch" =~ ^/opt/microsoft/msedge/[.]fieldbook-sandbox[.][a-zA-Z0-9]{10}$ ]]
[[ "$(/usr/bin/sudo /usr/bin/stat -c '%F:%u:%g:%a' "$root_scratch")" == 'directory:0:0:700' ]]
cleanup() {
    /usr/bin/sudo /usr/bin/rm -f -- "$root_scratch/helper"
    /usr/bin/sudo /usr/bin/rmdir -- "$root_scratch"
}
trap cleanup EXIT
/usr/bin/sudo /usr/bin/install --owner=root --group=root --mode=0755 -- "$helper" "$root_scratch/helper"
[[ "$(/usr/bin/sudo /usr/bin/stat -c '%F:%u:%g:%a' "$root_scratch/helper")" == 'regular file:0:0:755' ]]
printf '%s  %s\n' "$expected" "$root_scratch/helper" | /usr/bin/sudo /usr/bin/md5sum --check --status
[[ "$(/usr/bin/sudo /usr/bin/stat -c '%d' "$root_scratch/helper")" == "$(/usr/bin/stat -c '%d' "$helper")" ]]
# Never elevate the old writable inode: only promote the verified private copy.
/usr/bin/sudo /usr/bin/chmod 4755 "$root_scratch/helper"
/usr/bin/sudo /usr/bin/mv -T -- "$root_scratch/helper" "$helper"
[[ "$(/usr/bin/stat -c '%F:%u:%g:%a' "$helper")" == 'regular file:0:0:4755' ]]
for directory in /opt /opt/microsoft /opt/microsoft/msedge; do
    [[ "$(/usr/bin/stat -c '%u:%g:%a' "$directory")" == 0:0:755 ]]
done
printf 'Edge sandbox helper verified: packaged bytes, new root:root 4755 inode, protected ancestors\n'
