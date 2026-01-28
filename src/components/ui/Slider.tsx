import React from "react";

interface SliderProps {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (value: number) => void;
  disabled?: boolean;
  className?: string;
}

export const Slider: React.FC<SliderProps> = ({
  value,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  disabled = false,
  className = "",
}) => {
  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onChange(Number(e.target.value));
  };

  const percentage = ((value - min) / (max - min)) * 100;

  return (
    <div className={`relative flex items-center ${className}`}>
      <div className="relative w-full h-6">
        {/* Background track */}
        <div className="absolute inset-0 h-[4px] bg-mid-gray/20 rounded-full top-1/2 -translate-y-1/2" />
        
        {/* Filled track */}
        <div 
          className="absolute h-[4px] rounded-full top-1/2 -translate-y-1/2 transition-all duration-75 ease-out pointer-events-none"
          style={{ 
            width: `${percentage}%`,
            backgroundColor: 'var(--color-slider-fill)'
          }}
        />
        
        {/* Actual input */}
        <input
          data-tauri-drag-region="false"
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={handleChange}
          disabled={disabled}
          className="relative w-full h-6 bg-transparent appearance-none cursor-pointer focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed slider-custom z-10"
        />
      </div>
    </div>
  );
};
