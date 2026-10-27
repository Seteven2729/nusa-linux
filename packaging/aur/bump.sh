#!/bin/sh
# Point the AUR package at a new release: ./bump.sh 0.4.0 <sha256 of the release tarball>
# Run by the Release workflow; works the same by hand.
set -eu
ver=$1
sum=$2
cd "$(dirname "$0")"
old_ver=$(sed -n 's/^pkgver=//p' PKGBUILD)
old_sum=$(sed -n "s/^sha256sums=('\(.*\)')/\1/p" PKGBUILD)
sed -i "s/^pkgver=.*/pkgver=$ver/; s/^pkgrel=.*/pkgrel=1/; s/$old_sum/$sum/" PKGBUILD
# .SRCINFO mirrors PKGBUILD. makepkg --printsrcinfo refuses to run as root (CI containers),
# so patch the same fields here.
sed -i "s/pkgver = .*/pkgver = $ver/; s/pkgrel = .*/pkgrel = 1/; s|/v$old_ver/nusa-linux-v$old_ver-|/v$ver/nusa-linux-v$ver-|; s/$old_sum/$sum/" .SRCINFO
