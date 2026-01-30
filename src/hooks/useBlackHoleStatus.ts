import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface BlackHoleStatus {
  installed: boolean;
  paths: string[];
}

let cachedStatus: BlackHoleStatus | null = null;
let didFetch = false;
const listeners = new Set<(status: BlackHoleStatus | null) => void>();

const notify = () => {
  listeners.forEach((listener) => listener(cachedStatus));
};

export const useBlackHoleStatus = () => {
  const [status, setStatus] = useState<BlackHoleStatus | null>(cachedStatus);
  const [isLoading, setIsLoading] = useState(!cachedStatus);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const listener = (next: BlackHoleStatus | null) => {
      setStatus(next);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      const result = await invoke<BlackHoleStatus>("get_blackhole_status");
      cachedStatus = result;
      setStatus(result);
      notify();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to check status");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (didFetch) return;
    didFetch = true;
    refresh();
  }, [refresh]);

  return { status, isLoading, error, refresh };
};
