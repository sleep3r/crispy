import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// Mock Tauri APIs since tests run outside the Tauri runtime
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
