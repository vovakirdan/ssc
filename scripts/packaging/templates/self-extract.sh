#!/bin/sh
set -eu
umask 077
if [ "$(id -u)" = 0 ]; then echo 'Run this installer as your normal user, without sudo.' >&2; exit 1; fi
scratch=$(mktemp -d "${TMPDIR:-/tmp}/ssc-install.XXXXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
line=$(awk '/^__SSC_PAYLOAD_BELOW__$/ { print NR + 1; exit }' "$0")
[ -n "$line" ] || exit 1
tail -n +"$line" "$0" | tar -xzf - --no-same-owner -C "$scratch"
sh "$scratch/install.sh" "$scratch"
exit 0
__SSC_PAYLOAD_BELOW__
