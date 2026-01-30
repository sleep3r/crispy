import React from "react";

interface PathDisplayProps {
  path: string;
  onOpen: () => void;
  disabled?: boolean;
}

export const PathDisplay: React.FC<PathDisplayProps> = ({
  path,
  onOpen,
  disabled = false,
}) => {
  return (
    <div className="flex items-center gap-2">
      <div className="flex-1 min-w-0 px-2 py-2 bg-mid-gray/10 border border-mid-gray/20 rounded text-xs font-mono break-all select-text cursor-text">
        {path}
      </div>
      <button
        type="button"
        onClick={onOpen}
        disabled={disabled}
        className="px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        Open
      </button>
    </div>
  );
};
