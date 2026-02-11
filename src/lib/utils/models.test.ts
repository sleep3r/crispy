import { describe, it, expect } from "vitest";
import { sortModels, type TranscriptionModelInfo } from "./models";

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
    const models = [
      makeModel("large"),
      makeModel("turbo"),
      makeModel("medium"),
      makeModel("small"),
      makeModel("moonshine-base"),
      makeModel("parakeet-tdt-0.6b-v2"),
      makeModel("parakeet-tdt-0.6b-v3"),
    ];
    const sorted = sortModels(models);
    expect(sorted.map((m) => m.id)).toEqual([
      "parakeet-tdt-0.6b-v3",
      "parakeet-tdt-0.6b-v2",
      "moonshine-base",
      "small",
      "medium",
      "turbo",
      "large",
    ]);
  });
});
