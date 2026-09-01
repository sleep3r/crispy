import { describe, it, expect } from "vitest";
import { MODEL_ORDER, sortModels, type TranscriptionModelInfo } from "./models";

const makeModel = (
  id: string,
  name: string = id
): TranscriptionModelInfo => ({
  id,
  name,
  description: "",
  size_mb: 0,
  is_downloaded: false,
  is_downloading: false,
});

describe("sortModels", () => {
  it("sorts known models by their defined order", () => {
    const models = [
      makeModel("large", "Large"),
      makeModel("small", "Small"),
      makeModel("parakeet-tdt-0.6b-v3", "Parakeet v3"),
    ];
    const sorted = sortModels(models);
    expect(sorted.map((m) => m.id)).toEqual([
      "parakeet-tdt-0.6b-v3",
      "small",
      "large",
    ]);
  });

  it("places known models before unknown ones", () => {
    const models = [
      makeModel("custom-model", "Custom"),
      makeModel("medium", "Medium"),
    ];
    const sorted = sortModels(models);
    expect(sorted.map((m) => m.id)).toEqual(["medium", "custom-model"]);
  });

  it("sorts unknown models alphabetically by name", () => {
    const models = [
      makeModel("z-model", "Zebra"),
      makeModel("a-model", "Alpha"),
      makeModel("m-model", "Mike"),
    ];
    const sorted = sortModels(models);
    expect(sorted.map((m) => m.id)).toEqual([
      "a-model",
      "m-model",
      "z-model",
    ]);
  });

  it("does not mutate the original array", () => {
    const models = [
      makeModel("large", "Large"),
      makeModel("small", "Small"),
    ];
    const original = [...models];
    sortModels(models);
    expect(models).toEqual(original);
  });

  it("handles empty array", () => {
    expect(sortModels([])).toEqual([]);
  });

  it("handles single element", () => {
    const models = [makeModel("turbo", "Turbo")];
    expect(sortModels(models)).toEqual(models);
  });

  it("preserves full model order when all known models present", () => {
    const shuffled = [...MODEL_ORDER].reverse().map((id) => makeModel(id));
    const sorted = sortModels(shuffled);
    expect(sorted.map((m) => m.id)).toEqual(MODEL_ORDER);
  });

  it("orders every shipped model explicitly", () => {
    // A model missing from MODEL_ORDER silently drops to the alphabetical tail,
    // which reads as a UI bug rather than an omission. Keep this list in sync
    // with the registry in src-tauri/src/managers/model.rs.
    expect(MODEL_ORDER).toEqual([
      "parakeet-tdt-0.6b-v3",
      "parakeet-tdt-0.6b-v2",
      "moonshine-base",
      "moonshine-tiny-streaming-en",
      "moonshine-small-streaming-en",
      "moonshine-medium-streaming-en",
      "gigaam-v3-e2e-ctc",
      "gigaam-v3-e2e-rnnt",
      "gigaam-v3-rnnt",
      "gigaam-v3-ctc",
      "parakeet-unified-en",
      "qwen3-asr-0.6b",
      "canary-1b-flash",
      "sense-voice-int8",
      "canary-180m-flash",
      "canary-1b-v2",
      "cohere-int8",
      "small",
      "medium",
      "turbo",
      "large",
      "breeze-asr",
    ]);
  });
});
