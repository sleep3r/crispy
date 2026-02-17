import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PermissionStatus {
  microphone: "granted" | "denied" | "not_determined";
  screen_recording: boolean;
}

interface UsePermissionsResult {
  permissions: PermissionStatus | null;
  loading: boolean;
  allGranted: boolean;
  recheck: () => Promise<void>;
  requestPermission: (type: "microphone" | "screen_recording") => Promise<boolean>;
}

export function usePermissions(): UsePermissionsResult {
  const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const recheck = useCallback(async () => {
    try {
      setLoading(true);
      const status = await invoke<PermissionStatus>("check_permissions");
      setPermissions(status);
    } catch (err) {
      console.error("Failed to check permissions:", err);
      setPermissions({ microphone: "granted", screen_recording: true });
    } finally {
      setLoading(false);
    }
  }, []);

  const requestPermission = useCallback(async (type: "microphone" | "screen_recording") => {
    try {
      const granted = await invoke<boolean>("request_permission", { permissionType: type });
      // Recheck after request
      await recheck();
      return granted;
    } catch (err) {
      console.error("Failed to request permission:", err);
      return false;
    }
  }, [recheck]);

  // Initial check
  useEffect(() => {
    recheck();
  }, [recheck]);

  // Auto-poll every 3 seconds while permissions are not all granted
  useEffect(() => {
    const allOk = permissions !== null &&
      permissions.microphone === "granted" &&
      permissions.screen_recording === true;

    if (allOk) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }

    if (!pollRef.current && permissions !== null) {
      pollRef.current = setInterval(async () => {
        try {
          const status = await invoke<PermissionStatus>("check_permissions");
          setPermissions(status);
        } catch {
          // ignore
        }
      }, 3000);
    }

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [permissions]);

  const allGranted = permissions !== null &&
    permissions.microphone === "granted" &&
    permissions.screen_recording === true;

  return {
    permissions,
    loading,
    allGranted,
    recheck,
    requestPermission,
  };
}
