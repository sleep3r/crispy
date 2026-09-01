#!/usr/bin/env bash
# Mirror the transcription/diarization model artifacts into Crispy's own S3
# (bucket `crispy`, prefix `models/`, public read via https://s3.crispy.fyi/models/).
#
# Source of truth for the file list is MODELS[] below; it must stay in sync with
# src-tauri/src/managers/model.rs. Upstream artifacts come from Handy's CDN.
#
# Usage:
#   ./scripts/sync_models.sh            # download missing, verify, upload missing
#   ./scripts/sync_models.sh --check    # verify only, never write to S3
#
# Credentials are read from .env in the repo root (gitignored):
#   S3_ACCESS_KEY_ID, S3_SECRET_ACCESS_KEY, S3_REGION, S3_BUCKET, S3_ENDPOINT

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${MODEL_CACHE_DIR:-/tmp/crispy_models}"
HANDY_BASE="https://blob.handy.computer"
PUBLIC_BASE="https://s3.crispy.fyi/models"
ALIAS="crispy-sync"
CHECK_ONLY=0
[ "${1:-}" = "--check" ] && CHECK_ONLY=1

# filename <TAB> sha256. Empty sha256 = no upstream checksum published; skipped.
# sha256 values are Handy's own, from src-tauri/src/managers/model.rs.
MODELS=(
  "ggml-small.bin	1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"
  "whisper-medium-q4_1.bin	79283fc1f9fe12ca3248543fbd54b73292164d8df5a16e095e2bceeaaabddf57"
  "ggml-large-v3-turbo.bin	1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"
  "ggml-large-v3-q5_0.bin	d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1"
  "breeze-asr-q5_k.bin	8efbf0ce8a3f50fe332b7617da787fb81354b358c288b008d3bdef8359df64c6"
  "parakeet-v2-int8.tar.gz	ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991"
  "parakeet-v3-int8.tar.gz	43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77"
  "moonshine-base.tar.gz	04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8"
  "moonshine-tiny-streaming-en.tar.gz	465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf"
  "moonshine-small-streaming-en.tar.gz	dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5"
  "moonshine-medium-streaming-en.tar.gz	07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13"
  "sense-voice-int8.tar.gz	171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4"
  "giga-am-v3-int8.tar.gz	d872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b"
  "canary-180m-flash.tar.gz	6d9cfca6118b296e196eaedc1c8fa9788305a7b0f1feafdb6dc91932ab6e53f7"
  "canary-1b-v2.tar.gz	02305b2a25f9cf3e7deaffa7f94df00efa44f442cd55c101c2cb9c000f904666"
  "cohere-int8.tar.gz	ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0"
  # Diarization models — not hosted by Handy, already in our bucket. Verified, never re-uploaded.
  "segmentation-3.0.onnx	"
  "wespeaker_en_voxceleb_CAM++.onnx	"
)

command -v mc >/dev/null || { echo "mc (MinIO client) not found: brew install minio/stable/mc" >&2; exit 1; }

[ -f "$REPO_ROOT/.env" ] || { echo "Missing $REPO_ROOT/.env with S3_* credentials" >&2; exit 1; }
set -a; . "$REPO_ROOT/.env"; set +a
: "${S3_ACCESS_KEY_ID:?}" "${S3_SECRET_ACCESS_KEY:?}" "${S3_REGION:?}" "${S3_BUCKET:?}" "${S3_ENDPOINT:?}"

# Selectel denies GetBucketLocation, so mc falls back to an empty signing region
# and every request comes back AccessDenied. mc has no --region flag, so the
# region is written straight into the alias entry in ~/.mc/config.json.
mc alias set "$ALIAS" "https://${S3_ENDPOINT}" "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" --api s3v4 >/dev/null
MC_CONFIG="${MC_CONFIG_DIR:-$HOME/.mc}/config.json"
python3 - "$MC_CONFIG" "$ALIAS" "$S3_REGION" <<'PY'
import json, sys
path, alias, region = sys.argv[1:4]
with open(path) as fh:
    cfg = json.load(fh)
cfg["aliases"][alias]["region"] = region
with open(path, "w") as fh:
    json.dump(cfg, fh, indent=4)
PY

mkdir -p "$CACHE_DIR"
missing=(); mismatched=()

echo "Checking ${#MODELS[@]} artifacts against s3://${S3_BUCKET}/models/"
for entry in "${MODELS[@]}"; do
    file="${entry%%	*}"; want="${entry##*	}"
    if mc stat "$ALIAS/${S3_BUCKET}/models/$file" >/dev/null 2>&1; then
        printf '  ok       %s\n' "$file"
        continue
    fi
    printf '  MISSING  %s\n' "$file"
    missing+=("$entry")
done

if [ ${#missing[@]} -eq 0 ]; then
    echo "All artifacts present."
    exit 0
fi

if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "${#missing[@]} missing (check-only, nothing uploaded)."
    exit 1
fi

echo
echo "Fetching ${#missing[@]} artifact(s) from $HANDY_BASE into $CACHE_DIR"
for entry in "${missing[@]}"; do
    file="${entry%%	*}"; want="${entry##*	}"
    if [ -z "$want" ]; then
        echo "  !! $file has no upstream source and is absent from S3 — restore it manually" >&2
        exit 1
    fi
    if [ ! -s "$CACHE_DIR/$file" ]; then
        echo "  downloading $file"
        curl -fL --retry 3 --progress-bar -o "$CACHE_DIR/$file" "$HANDY_BASE/$file"
    fi
    got="$(shasum -a 256 "$CACHE_DIR/$file" | awk '{print $1}')"
    if [ "$got" != "$want" ]; then
        echo "  !! sha256 mismatch for $file" >&2
        echo "     want $want" >&2
        echo "     got  $got" >&2
        mismatched+=("$file")
    else
        echo "  sha256 ok $file"
    fi
done
[ ${#mismatched[@]} -eq 0 ] || { echo "Refusing to upload: ${#mismatched[@]} checksum failure(s)." >&2; exit 1; }

echo
echo "Uploading to s3://${S3_BUCKET}/models/"
for entry in "${missing[@]}"; do
    file="${entry%%	*}"
    mc cp "$CACHE_DIR/$file" "$ALIAS/${S3_BUCKET}/models/$file"
done

echo
echo "Verifying public reads"
fail=0
for entry in "${missing[@]}"; do
    file="${entry%%	*}"
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 30 -I "$PUBLIC_BASE/$file")"
    printf '  %s  %s\n' "$code" "$PUBLIC_BASE/$file"
    [ "$code" = "200" ] || fail=1
done
[ "$fail" -eq 0 ] || { echo "Some artifacts are not publicly readable." >&2; exit 1; }

echo
echo "Done. Cache kept at $CACHE_DIR (delete it to reclaim disk)."
