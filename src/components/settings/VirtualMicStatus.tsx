import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { SettingContainer } from '../ui/SettingContainer';

interface VirtualMicStatus {
  plugin_installed: boolean;
  plugin_path: string;
  shared_memory_active: boolean;
}

export function VirtualMicStatus() {
  const [status, setStatus] = useState<VirtualMicStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchStatus = async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<VirtualMicStatus>('get_virtual_mic_status');
      setStatus(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch status');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();
    // Refresh status every 5 seconds
    const interval = setInterval(fetchStatus, 5000);
    return () => clearInterval(interval);
  }, []);

  if (loading && !status) {
    return (
      <SettingContainer label="Virtual Microphone">
        <div className="text-sm text-gray-400">Loading...</div>
      </SettingContainer>
    );
  }

  if (error) {
    return (
      <SettingContainer label="Virtual Microphone">
        <div className="text-sm text-red-400">Error: {error}</div>
      </SettingContainer>
    );
  }

  if (!status) {
    return null;
  }

  return (
    <SettingContainer label="Virtual Microphone">
      <div className="space-y-2 text-sm">
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${status.plugin_installed ? 'bg-green-500' : 'bg-red-500'}`} />
          <span className="text-gray-300">
            Plugin: {status.plugin_installed ? 'Installed' : 'Not installed'}
          </span>
        </div>
        
        {status.plugin_installed && (
          <div className="flex items-center gap-2">
            <div className={`w-2 h-2 rounded-full ${status.shared_memory_active ? 'bg-green-500' : 'bg-yellow-500'}`} />
            <span className="text-gray-300">
              Audio: {status.shared_memory_active ? 'Active' : 'Inactive'}
            </span>
          </div>
        )}
        
        {!status.plugin_installed && (
          <div className="mt-2 p-2 bg-yellow-900/20 border border-yellow-700/30 rounded text-xs text-yellow-200">
            <div className="font-semibold mb-1">Installation required</div>
            <div className="text-yellow-300/80">
              Run in terminal: <code className="bg-black/30 px-1 rounded">make plugin-install</code>
            </div>
          </div>
        )}
        
        {status.plugin_installed && !status.shared_memory_active && (
          <div className="mt-2 p-2 bg-blue-900/20 border border-blue-700/30 rounded text-xs text-blue-200">
            <div className="text-blue-300/80">
              Start monitoring a microphone to activate the virtual mic output
            </div>
          </div>
        )}
        
        {status.plugin_installed && status.shared_memory_active && (
          <div className="mt-2 p-2 bg-green-900/20 border border-green-700/30 rounded text-xs text-green-200">
            <div className="text-green-300/80">
              "Crispy Microphone" is available in system audio input devices
            </div>
          </div>
        )}
      </div>
    </SettingContainer>
  );
}
