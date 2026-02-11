import { describe, it, expect } from "vitest";
import { formatModelSize } from "./format";

describe("formatModelSize", () => {
  it("returns dash for null", () => {
    expect(formatModelSize(null)).toBe("—");
  });

  it("returns dash for undefined", () => {
    expect(formatModelSize(undefined)).toBe("—");
  });

  it("returns dash for zero", () => {
    expect(formatModelSize(0)).toBe("—");
  });

  it("returns dash for negative values", () => {
    expect(formatModelSize(-100)).toBe("—");
  });

  it("returns dash for NaN", () => {
    expect(formatModelSize(NaN)).toBe("—");
  });

  it("returns dash for Infinity", () => {
    expect(formatModelSize(Infinity)).toBe("—");
  });

  it("formats small MB values with one decimal", () => {
    expect(formatModelSize(50)).toBe("50.0 MB");
  });

  it("formats MB values >= 100 with no decimals", () => {
    expect(formatModelSize(150)).toBe("150 MB");
  });

  it("formats values >= 1024 MB as GB with one decimal", () => {
    expect(formatModelSize(1536)).toBe("1.5 GB");
  });

  it("formats GB values >= 10 with no decimals", () => {
    expect(formatModelSize(10240)).toBe("10 GB");
  });

  it("formats exactly 1024 MB as 1.0 GB", () => {
    expect(formatModelSize(1024)).toBe("1.0 GB");
  });

  it("formats fractional MB values", () => {
    expect(formatModelSize(99.9)).toBe("99.9 MB");
  });
});
