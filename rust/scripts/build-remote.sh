#!/bin/bash
# Builds the Linux and Windows release files on another machine with Docker
# (e.g. a fast desktop) and copies them back to target/dist/.
#
#   scripts/build-remote.sh <ssh-host>
#
# The source goes to ~/padbridge-build on that machine (replaced each time);
# it needs Docker and bash, nothing else. The macOS app is built on a Mac with
# scripts/bundle-macos.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

HOST="${1:?usage: scripts/build-remote.sh <ssh-host>}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NAME="padbridge-$VERSION-linux-x86_64"

rsync -az --delete --exclude target --exclude .DS_Store ./ "$HOST:padbridge-build/"

# The script goes over stdin, so the login shell there may be anything.
ssh "$HOST" bash -l -s <<REMOTE
set -euo pipefail
cd ~/padbridge-build
scripts/build-linux.sh
WIN=\$(scripts/build-windows.sh | tail -1)
rm -rf target/dist
mkdir -p target/dist/$NAME
cp target/linux-portable/release/padbridge app/padbridge.service target/dist/$NAME/
cp "\$WIN" target/dist/padbridge-$VERSION-windows-x86_64.exe
REMOTE

mkdir -p target/dist
rsync -a --delete "$HOST:padbridge-build/target/dist/" target/dist/
# Docs and licence live one level above rust/.
cp ../DOC.md ../README.md ../LICENSE "target/dist/$NAME/"

# Neutral owner in the archive (not the build user), no macOS metadata.
cd target/dist
if tar --version 2>/dev/null | grep -q bsdtar; then
    COPYFILE_DISABLE=1 tar --no-xattrs --uid 1000 --gid 1000 --uname padbridge --gname padbridge -czf "$NAME.tar.gz" "$NAME"
else
    tar --owner=padbridge:1000 --group=padbridge:1000 -czf "$NAME.tar.gz" "$NAME"
fi
echo "target/dist:"
ls -l
