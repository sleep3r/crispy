import { useEffect, useRef } from "react";
import { listen, type UnlistenFn, type Event } from "@tauri-apps/api/event";

/**
 * Safe hook for Tauri event listeners that properly handles:
 * - React StrictMode double-mount
 * - Navigation remounting
 * - Cleanup-before-registration races
 * - Double-unlisten errors
 */
export function useTauriListen<T>(
  eventName: string,
  handler: (event: Event<T>) => void
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    (async () => {
      try {
        const u = await listen<T>(eventName, (e) => handlerRef.current(e));
        if (cancelled) {
          // If we unmounted before listen resolved, immediately undo
          await u();
          return;
        }
        unlisten = u;
      } catch (err) {
        console.error(`listen(${eventName}) failed:`, err);
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) {
        // IMPORTANT: only call once, and only if we actually have it
        try {
          unlisten();
        } catch (e) {
          console.warn(`unlisten(${eventName}) failed:`, e);
        }
      }
    };
  }, [eventName]);
}
