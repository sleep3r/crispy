export interface TranscriptionModelInfo {
  id: string;
  name: string;
  description: string;
  size_mb: number;
  is_downloaded: boolean;
  is_downloading: boolean;
}

export const MODEL_ORDER = [
  "parakeet-tdt-0.6b-v3",
  "parakeet-tdt-0.6b-v2",
  "moonshine-base",
  "small",
  "medium",
  "turbo",
  "large",
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
