import React from "react";

const ModelSelector: React.FC = () => {
  return (
    <div className="flex items-center gap-2 px-3 py-1.5 bg-mid-gray/5 rounded-md border border-mid-gray/10">
      <div className="w-2 h-2 rounded-full bg-green-500"></div>
      <span className="text-xs font-medium">Dummy Model</span>
    </div>
  );
};

export default ModelSelector;
