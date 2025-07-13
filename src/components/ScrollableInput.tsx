import React, { useRef } from "react";
import { cn } from "@/lib/utils";
import ScrollVelocity from "./ScrollVelocity";

interface ScrollableInputProps {
  value: string;
  readOnly?: boolean;
  className?: string;
  placeholder?: string;
  velocity?: number;
  damping?: number;
  stiffness?: number;
  numCopies?: number;
  onCopy?: () => void;
  copyButton?: React.ReactNode;
}

const ScrollableInput: React.FC<ScrollableInputProps> = ({
  value,
  readOnly = false,
  className = "",
  placeholder = "",
  velocity = 50,
  damping = 50,
  stiffness = 400,
  numCopies = 3,
  onCopy,
  copyButton,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);

  return (
    <div
      ref={containerRef}
      className={cn(
        "flex w-full rounded-md border border-input bg-background px-3 py-2 text-base ring-offset-background focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 md:text-sm h-10 items-center overflow-hidden",
        className
      )}
    >
      <div className="flex-1 min-w-0">
        {value ? (
          <ScrollVelocity
            scrollContainerRef={containerRef}
            texts={[value]}
            velocity={velocity}
            damping={damping}
            stiffness={stiffness}
            numCopies={numCopies}
            className="text-foreground text-sm"
            parallaxClassName="h-full flex items-center"
            scrollerClassName="h-full flex items-center"
            parallaxStyle={{ height: '100%' }}
            scrollerStyle={{ height: '100%' }}
          />
        ) : (
          <span className="text-muted-foreground text-sm">{placeholder}</span>
        )}
      </div>
      {copyButton && (
        <div className="ml-2 flex-shrink-0">
          {copyButton}
        </div>
      )}
    </div>
  );
};

export default ScrollableInput; 