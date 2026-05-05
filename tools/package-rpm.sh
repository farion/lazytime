#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 5 ]]; then
  echo "usage: $0 <binary> <version> <arch> <variant> <outdir>" >&2
  exit 1
fi

BIN_PATH="$1"
VERSION="$2"
ARCH="$3"
VARIANT="$4"
OUTDIR="$5"

ARCH_RPM="$ARCH"
if [[ "$ARCH" == "x86_64" ]]; then
  ARCH_RPM="x86_64"
fi

PKG_NAME="lazytime"
ARTIFACT_BASE="lazytime-${VERSION}-linux-${ARCH}-${VARIANT}"
RPM_PATH="${OUTDIR}/${ARTIFACT_BASE}.rpm"

fpm \
  --input-type dir \
  --output-type rpm \
  --name "${PKG_NAME}" \
  --version "${VERSION}" \
  --architecture "${ARCH_RPM}" \
  --package "${RPM_PATH}" \
  --description "Rule-driven automatic time tracking assistant" \
  --license "MIT" \
  --maintainer "lazytime" \
  --prefix /usr/local \
  "${BIN_PATH}=/bin/lazytime"

echo "${RPM_PATH}"
