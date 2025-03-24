// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useRef, useEffect } from "react";
interface CarouselThumbsProps {
  images: string[] | undefined;
  currentIndex: number;
  changeIndex: (newIndex: number) => void;
  height?: number | string;
  width?: number | string;
  thumbsClassName?: string;
  containerClassName?: string;
  thumbStyle?: React.CSSProperties; // Add this prop
  containerStyle?: React.CSSProperties; // Add this prop
}

const CarouselThumbs: React.FC<CarouselThumbsProps> = ({
  images,
  currentIndex,
  changeIndex,
  thumbsClassName,
  containerClassName,
  thumbStyle, // Add this prop
  containerStyle, // Add this prop
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    if (containerRef.current && itemRefs.current[currentIndex]) {
      const container = containerRef.current;
      const activeItem = itemRefs.current[currentIndex];

      const containerRect = container.getBoundingClientRect();
      const activeItemRect = activeItem.getBoundingClientRect();

      const scrollOffset =
        activeItem.offsetLeft -
        container.offsetLeft -
        (containerRect.width - activeItemRect.width) / 2;

      container.scrollTo({
        left: scrollOffset,
        behavior: "smooth",
      });
    }
  }, [currentIndex]);
  if (!images) return null;
  return (
    <div className="flex flex-row items-center py-4">
      <div
        ref={containerRef}
        className={containerClassName}
        style={{
          ...containerStyle, // Add this line
        }}
      >
        {images?.map((image, index) => (
          <img
            className={thumbsClassName}
            key={index}
            ref={(el) => (itemRefs.current[index] = el)}
            src={image}
            alt={`carousel-thumb-${index}`}
            onClick={() => changeIndex(index)}
            style={{
              border:
                currentIndex === index
                  ? "4px solid var(--sui-blue-bright)"
                  : "4px solid transparent",
            }}
          />
        ))}
      </div>
    </div>
  );
};

export default CarouselThumbs;
