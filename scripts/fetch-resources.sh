#!/usr/bin/env bash
# Fetch runtime resources that are bundled with the app but not committed.
# Currently: the Silero VAD v4 model used to trim silence.
set -euo pipefail

cd "$(dirname "$0")/.."
dest="src-tauri/resources/models/silero_vad_v4.onnx"

if [[ -s "$dest" ]]; then
  echo "Already present: $dest"
  exit 0
fi

urls=(
  "https://blob.handy.computer/silero_vad_v4.onnx"
  "https://github.com/snakers4/silero-vad/raw/v4.0/files/silero_vad.onnx"
)
for url in "${urls[@]}"; do
  echo "Downloading $url"
  if curl -fL --retry 3 -o "$dest.tmp" "$url"; then
    # The v4 model is ~1.8 MB; anything tiny is an error page.
    if [[ $(wc -c < "$dest.tmp") -gt 1000000 ]]; then
      mv "$dest.tmp" "$dest"
      echo "Saved $dest"
      exit 0
    fi
  fi
  rm -f "$dest.tmp"
done
echo "Could not download the Silero VAD model; EchoLocal will use its built-in energy VAD." >&2
exit 1
