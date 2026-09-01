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

## Upstream

The transcription artifacts originate from Handy's CDN
(`https://blob.handy.computer/<file>`, MIT). We mirror rather than hot-link so
the app does not depend on someone else's bucket staying up. The sha256 values in
`sync_models.sh` are Handy's own, taken from their `model.rs`, and every mirrored
object has been verified against them.

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
