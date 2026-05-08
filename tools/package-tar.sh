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

ARTIFACT_BASE="lazytime-${VERSION}-linux-${ARCH}-${VARIANT}"
STAGE_DIR="${OUTDIR}/${ARTIFACT_BASE}"

mkdir -p \
  "${STAGE_DIR}/bin" \
  "${STAGE_DIR}/share/doc/lazytime" \
  "${STAGE_DIR}/share/icons/hicolor/512x512/apps" \
  "${STAGE_DIR}/share/icons/hicolor/scalable/apps" \
  "${STAGE_DIR}/share/applications"
cp "${BIN_PATH}" "${STAGE_DIR}/bin/lazytime"
cp README.md "${STAGE_DIR}/share/doc/lazytime/README.md"
cp icon_black.png "${STAGE_DIR}/share/icons/hicolor/512x512/apps/lazytime.png"
cp icon_black.svg "${STAGE_DIR}/share/icons/hicolor/scalable/apps/lazytime.svg"
cp icon_white.png "${STAGE_DIR}/share/icons/hicolor/512x512/apps/lazytime-white.png"
cp icon_white.svg "${STAGE_DIR}/share/icons/hicolor/scalable/apps/lazytime-white.svg"
cp packaging/com.lazytime.app.desktop "${STAGE_DIR}/share/applications/com.lazytime.app.desktop"

if [[ -f LICENSE ]]; then
  cp LICENSE "${STAGE_DIR}/share/doc/lazytime/LICENSE"
fi

tar -C "${OUTDIR}" -czf "${OUTDIR}/${ARTIFACT_BASE}.tar.gz" "${ARTIFACT_BASE}"
echo "${OUTDIR}/${ARTIFACT_BASE}.tar.gz"
