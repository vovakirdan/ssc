#!/bin/sh
set -eu
umask 077
payload=$1
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
app="$data/ssc/app"
profile=${SSC_PROFILE_DIR:-"$data/com.vladimirkirdan.ssc"}
case "$data" in /*) ;; *) echo 'Application data path must be absolute' >&2; exit 1;; esac
case "$profile" in /*) ;; *) echo 'Profile path must be absolute' >&2; exit 1;; esac
[ -d "$HOME" ] && [ ! -L "$profile" ] && [ ! -L "$data/ssc" ] && [ ! -L "$app" ] || exit 1
mkdir -p "$app" "$profile" "$data/applications"
chmod 700 "$data/ssc" "$app" "$profile"
cp "$payload/ssc.AppImage" "$app/ssc.AppImage.new"
chmod 700 "$app/ssc.AppImage.new"
mv -f "$app/ssc.AppImage.new" "$app/ssc.AppImage"
if [ ! -e "$profile/access.active" ]; then
  cp "$payload/bootstrap.json" "$profile/bootstrap.json.new"
  chmod 600 "$profile/bootstrap.json.new"
  mv -f "$profile/bootstrap.json.new" "$profile/bootstrap.json"
fi
cat > "$app/launch" <<'LAUNCH'
#!/bin/sh
set -eu
APPIMAGE_EXTRACT_AND_RUN=1
export APPIMAGE_EXTRACT_AND_RUN
exec "$(dirname "$0")/ssc.AppImage" "$@"
LAUNCH
chmod 700 "$app/launch"
# Desktop Exec uses quoted arguments; a newline or shell metacharacter is not evaluated.
case "$app" in *'"'*|*'`'*|*'$'*|*'%'*|*'\\'*) echo 'Unsupported installation path' >&2; exit 1;; esac
cat > "$data/applications/ssc.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=SSC
Comment=Secret Chat
Exec="$app/launch" %U
Terminal=false
Categories=Network;InstantMessaging;
DESKTOP
chmod 600 "$data/applications/ssc.desktop"
cat > "$app/uninstall" <<'UNINSTALL'
#!/bin/sh
set -eu
installed_app=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
data=$(dirname -- "$(dirname -- "$installed_app")")
rm -f "$data/applications/ssc.desktop"
rm -f "$installed_app/ssc.AppImage" "$installed_app/launch" "$installed_app/uninstall"
rmdir "$installed_app" "$(dirname -- "$installed_app")" 2>/dev/null || true
printf '%s\n' 'SSC removed. Your access profile was retained.'
UNINSTALL
chmod 700 "$app/uninstall"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" >/dev/null 2>&1 || true
printf '%s\n' 'SSC installed. Open SSC from your applications menu.'
