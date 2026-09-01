# Model hosting

Crispy downloads every transcription and diarization model from its own bucket,
`https://s3.crispy.fyi/models/<file>` (Selectel S3, bucket `crispy`, prefix
`models/`). Nothing is fetched from a third-party CDN at runtime.

The registry in `src-tauri/src/managers/model.rs` is the source of truth for what
the app offers; `scripts/sync_models.sh` is the source of truth for what must
exist in the bucket. Keep the two in sync — a registry entry whose object is
missing turns into a download failure with a confusing error.

## Syncing

```bash
./scripts/sync_models.sh --check   # verify every artifact exists, write nothing
./scripts/sync_models.sh           # download what is missing, verify sha256, upload
```

Credentials come from `.env` in the repo root (gitignored):

```
S3_ACCESS_KEY_ID=...
S3_SECRET_ACCESS_KEY=...
S3_REGION=ru-3
S3_BUCKET=crispy
S3_ENDPOINT=s3.ru-3.storage.selcloud.ru
S3_PORT=443
```

The script refuses to upload anything whose sha256 does not match the value
recorded in `MODELS[]`, then re-checks each new object over the public URL.

### The region gotcha

Selectel denies `GetBucketLocation`. `mc` therefore falls back to an empty
signing region and **every** request comes back `AccessDenied` — including reads,
which makes it look like the credentials are wrong when they are fine. `mc` has
no `--region` flag, so `sync_models.sh` writes the region directly into the alias
entry in `~/.mc/config.json`. If you configure `mc` by hand, do the same:

```bash
mc alias set crispy "https://$S3_ENDPOINT" "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" --api s3v4
python3 -c "
import json; p='$HOME/.mc/config.json'; d=json.load(open(p))
d['aliases']['crispy']['region']='ru-3'; json.dump(d, open(p,'w'), indent=4)"
mc ls crispy/crispy/models/
```

## Two kinds of model

**ONNX** (`.tar.gz`, `is_directory: true`) run through `transcribe-rs`: Parakeet,
Moonshine, GigaAM, SenseVoice, Canary, Cohere. The tarball holds one top-level
directory that the extractor flattens to `filename`.

**GGML/GGUF** (`.bin`, `.gguf`, `is_directory: false`) run through
`transcribe-cpp`, which reads the architecture out of the file — one
`EngineType::TranscribeCpp` covers Whisper, GigaAM, Parakeet, Canary, Qwen3-ASR
and the rest. These are plain single-file downloads; nothing to extract.

> **Never re-enable the `whisper-cpp` feature on `transcribe-rs`.** It and
> `transcribe-cpp` each vendor their own ggml (0.9.5 vs 0.20.2) and export the
> same symbols. Their cargo `links` keys differ, so cargo accepts both and
> everything compiles, links and launches — but only one ggml survives and ~46 of
> ~100 `GGML_OP` ordinals then mean different operations, producing garbage
> transcripts with no error anywhere. The `no-duplicate-ggml` CI job is the only
> thing that catches this.

Some GGUF models cap how much audio they accept per call (`max_audio_ms` in their
capabilities — GigaAM allows 25 s, under our default 30 s chunk). The transcription
manager reads that limit at load time and shrinks the chunk accordingly; a new
model with a tighter limit needs no code change.

## Upstream

The artifacts originate from Handy's CDN (`https://blob.handy.computer`, MIT). We
mirror rather than hot-link so the app does not depend on someone else's bucket
staying up. Legacy artifacts sit at a flat prefix; catalog GGUFs are served from
`{repo_id}/{revision}/{filename}`, which is the third field in `MODELS[]`. The
sha256 values are Handy's own — from their `model.rs` and `catalog/catalog.json` —
and every mirrored object has been verified against them.

**Licensing.** Handy's catalog carries a `license` field, and 8 of its 69 models
are not permissive (7 `other`, and `canary-1b` is `cc-by-nc-4.0`, i.e.
non-commercial). We re-publish weights under our own domain, so check the terms
before mirroring anything that is not MIT / Apache-2.0 / CC-BY-4.0. Everything
currently in `MODELS[]` is permissive.

The two diarization models (`segmentation-3.0.onnx`,
`wespeaker_en_voxceleb_CAM++.onnx`) are **not** hosted by Handy and have no
upstream URL in the script. They exist only in our bucket — do not delete them,
as a bucket rebuild cannot recreate them and diarization would silently stop
working.

## Adding a model

1. Add the artifact and its sha256 to `MODELS[]` in `scripts/sync_models.sh`.
2. Run `./scripts/sync_models.sh` to mirror it.
3. Add a `ModelInfo` entry in `src-tauri/src/managers/model.rs` pointing at
   `https://s3.crispy.fyi/models/<file>`.
4. Add the model id to `MODEL_ORDER` in `src/lib/utils/models.ts` and to the
   assertion in `src/lib/utils/models.test.ts`, or it sorts into the
   alphabetical tail.
5. `size_mb` is display-only and is rendered as MiB — use the object's real
   `Content-Length / 1024 / 1024`, not the vendor's marketing number.

Directory models (`is_directory: true`) ship as a `.tar.gz` with a single
top-level directory, which the extractor flattens to `filename`. Keep `int8` out
of `filename` unless the bundle really contains `model.int8.onnx`; the
quantization selector in `transcription.rs` keys off that substring.
