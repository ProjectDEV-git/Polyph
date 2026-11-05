#!/bin/sh
# Installs Polyph from a GitHub release into ~/.local, with no root needed
# (so it works on immutable systems such as Bazzite and Fedora Atomic).
#
#   curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
#
# Settings (environment):
#   POLYPH_VERSION   a tag such as v0.1.0 (default: the latest release)
#   POLYPH_PREFIX    where bin/ and share/ go (default: ~/.local)
#   POLYPH_REPO      owner/name on GitHub (default: ProjectDEV-git/Polyph)
#   POLYPH_INSTALL_BASE  a URL holding the archives and SHA256SUMS
#                    (a mirror, or a local server for testing)
set -eu

REPO=${POLYPH_REPO:-ProjectDEV-git/Polyph}
VERSION=${POLYPH_VERSION:-latest}
PREFIX=${POLYPH_PREFIX:-$HOME/.local}

say() { printf '%s\n' "$*" >&2; }
die() { say "polyph install: $*"; exit 1; }

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
  final_url() { curl -fsSLI -o /dev/null -w '%{url_effective}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
  final_url() { wget -q --max-redirect=5 -S -O /dev/null "$1" 2>&1 | sed -n 's/^ *[Ll]ocation: *//p' | tail -n1; }
else
  die "needs curl or wget"
fi

case "$(uname -s)" in
  Linux) os=unknown-linux-gnu ;;
  Darwin) os=apple-darwin ;;
  *) die "unsupported system $(uname -s); on Windows use scripts/install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported processor $(uname -m); build from source instead" ;;
esac
if [ "$os" = unknown-linux-gnu ] && ldd --version 2>&1 | grep -qi musl; then
  die "musl-based systems (such as Alpine) need a build from source"
fi
target=$arch-$os

if [ -z "${POLYPH_INSTALL_BASE:-}" ]; then
  if [ "$VERSION" = latest ]; then
    url=$(final_url "https://github.com/$REPO/releases/latest") || die "cannot reach GitHub"
    VERSION=${url##*/}
    case "$VERSION" in v*) ;; *) die "no release found at github.com/$REPO" ;; esac
  fi
  base="https://github.com/$REPO/releases/download/$VERSION"
else
  base=$POLYPH_INSTALL_BASE
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "cannot download $base/SHA256SUMS"
archive=$(awk -v t="$target" '$2 ~ "^polyph-.*-" t "\\.tar\\.gz$" { print $2; exit }' "$tmp/SHA256SUMS")
[ -n "$archive" ] || die "this release has no build for $target"
say "Downloading $archive"
fetch "$base/$archive" "$tmp/$archive" || die "cannot download $base/$archive"

want=$(awk -v a="$archive" '$2 == a { print $1 }' "$tmp/SHA256SUMS")
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$archive" | awk '{ print $1 }')
else
  got=$(shasum -a 256 "$tmp/$archive" | awk '{ print $1 }')
fi
[ "$want" = "$got" ] || die "checksum mismatch for $archive (expected $want, got $got)"

tar -xzf "$tmp/$archive" -C "$tmp"
src="$tmp/${archive%.tar.gz}"
mkdir -p "$PREFIX/bin" "$PREFIX/share"
for b in polyph polyph-relay; do
  # Replace by rename so a running daemon keeps its old file.
  cp "$src/bin/$b" "$PREFIX/bin/.$b.new"
  chmod 755 "$PREFIX/bin/.$b.new"
  mv -f "$PREFIX/bin/.$b.new" "$PREFIX/bin/$b"
done
rm -rf "$PREFIX/share/polyph.new"
cp -R "$src/share/polyph" "$PREFIX/share/polyph.new"
rm -rf "$PREFIX/share/polyph"
mv "$PREFIX/share/polyph.new" "$PREFIX/share/polyph"

say "Installed $("$PREFIX/bin/polyph" --version) to $PREFIX/bin"
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) say "Add $PREFIX/bin to your PATH, for example: echo 'export PATH=\"$PREFIX/bin:\$PATH\"' >> ~/.profile" ;;
esac
say "If a daemon was running, restart it: polyph daemon stop"
say "Later: polyph update to update, polyph uninstall to remove."
