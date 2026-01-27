import React from "react";

interface HandyTextLogoProps {
  width?: number | string;
  className?: string;
  [key: string]: any;
}

const HandyTextLogo: React.FC<HandyTextLogoProps> = ({
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
        fontSize="20"
        fontWeight="bold"
        fontFamily="sans-serif"
      >
        CRISPY
      </text>
    </svg>
  );
};

export default HandyTextLogo;
