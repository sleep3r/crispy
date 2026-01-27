import React, { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import ModelSelector from "../model-selector/ModelSelector";

const Footer: React.FC = () => {
  const [version, setVersion] = useState("0.1.0");

  useEffect(() => {
    getVersion().then(setVersion).catch(console.error);
  }, []);

  return (
    <div className="w-full border-t border-mid-gray/20 bg-background">
      <div className="flex justify-between items-center text-xs px-4 py-2 text-mid-gray">
        <div className="flex items-center gap-4">
          <ModelSelector />
        </div>

        <div className="flex items-center gap-1">
           <span>v{version}</span>
        </div>
      </div>
    </div>
  );
};

export default Footer;
