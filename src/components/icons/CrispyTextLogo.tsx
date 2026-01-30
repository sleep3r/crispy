import React from "react";

interface CrispyTextLogoProps {
  width?: number | string;
  className?: string;
  [key: string]: any;
}

const CrispyTextLogo: React.FC<CrispyTextLogoProps> = ({
  width = 100,
  className = "",
  ...props
}) => {
  return (
    <svg
      width={width}
      viewBox="0 0 100 30"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...props}
    >
      <text
        x="50%"
        y="50%"
        dominantBaseline="middle"
        textAnchor="middle"
        fill="currentColor"
        fontSize="24"
        fontWeight="900"
        fontFamily="Outfit, system-ui, sans-serif"
      >
        CRISPY
      </text>
    </svg>
  );
};

export default CrispyTextLogo;
