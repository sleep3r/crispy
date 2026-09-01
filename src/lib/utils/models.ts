export interface TranscriptionModelInfo {
  id: string;
  name: string;
  description: string;
  size_mb: number;
  is_downloaded: boolean;
  is_downloading: boolean;
}

// Grouped by engine family, fastest-first within a family. Anything absent falls
// to the alphabetical tail, so every shipped model is listed here on purpose —
// otherwise half the list sorts by display name and the ordering looks arbitrary.
export const MODEL_ORDER = [
  "parakeet-tdt-0.6b-v3",
  "parakeet-tdt-0.6b-v2",
  "moonshine-base",
  "moonshine-tiny-streaming-en",
  "moonshine-small-streaming-en",
  "moonshine-medium-streaming-en",
  "gigaam-v3-e2e-ctc",
  "sense-voice-int8",
  "canary-180m-flash",
  "canary-1b-v2",
  "cohere-int8",
  "small",
  "medium",
  "turbo",
  "large",
  "breeze-asr",
];

export const sortModels = (list: TranscriptionModelInfo[]) => {
  const orderIndex = new Map(MODEL_ORDER.map((id, i) => [id, i]));
  return [...list].sort((a, b) => {
    const ai = orderIndex.get(a.id);
    const bi = orderIndex.get(b.id);
    if (ai !== undefined && bi !== undefined) return ai - bi;
    if (ai !== undefined) return -1;
    if (bi !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });
};
