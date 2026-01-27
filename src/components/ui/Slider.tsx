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
    <div className={`relative flex items-center h-6 ${className}`}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={handleChange}
        disabled={disabled}
        className="w-full h-1 bg-mid-gray/20 rounded-lg appearance-none cursor-pointer focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
        style={{
          background: `linear-gradient(to right, var(--color-text) ${percentage}%, rgba(128, 128, 128, 0.2) ${percentage}%)`,
        }}
      />
      <style>{`
        input[type=range]::-webkit-slider-thumb {
          -webkit-appearance: none;
          height: 16px;
          width: 16px;
          border-radius: 50%;
          background: var(--color-text);
          cursor: pointer;
          margin-top: -6px; /* You need to specify a margin in Chrome, but in Firefox and IE it is automatic */
          box-shadow: 0 1px 3px rgba(0,0,0,0.3);
        }
        input[type=range]::-webkit-slider-runnable-track {
          width: 100%;
          height: 4px;
          cursor: pointer;
          background: transparent;
          border-radius: 2px;
        }
      `}</style>
    </div>
  );
};
