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
      <input
        data-tauri-drag-region="false"
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={handleChange}
        disabled={disabled}
        className="w-full h-4 rounded-lg appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-mid-gray disabled:opacity-50 disabled:cursor-not-allowed slider-custom"
        style={{
          background: `linear-gradient(to right, var(--color-text) 0%, var(--color-text) ${percentage}%, rgba(128, 128, 128, 0.2) ${percentage}%, rgba(128, 128, 128, 0.2) 100%)`,
        }}
      />
    </div>
  );
};
