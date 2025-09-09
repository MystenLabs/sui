// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect, useRef } from "react";
import CarouselThumbs from "./CarouselThumbs";
import LiteYouTubeEmbed from "react-lite-youtube-embed";
import "react-lite-youtube-embed/dist/LiteYouTubeEmbed.css";

const LeftChevron = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 80 80"
    width="80"
    height="80"
    fill="none"
    style={{
      transition: "filter 0.3s ease-in-out",
    }}
    className="chevron"
  >
    <path
      d="M50 20 L30 40 L50 60"
      stroke="currentColor"
      strokeWidth="8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const RightChevron = () => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 80 80"
    width="80"
    height="80"
    fill="none"
    style={{
      transition: "filter 0.3s ease-in-out",
    }}
    className="chevron"
  >
    <path
      d="M30 20 L50 40 L30 60"
      stroke="currentColor"
      strokeWidth="8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

export default function YTCarousel(props) {
  const { ids } = props;

  const thumbs = (ids: string[]): string[] => {
    const thumbUrls: string[] = [];
    ids.map((id) =>
      thumbUrls.push(`https://img.youtube.com/vi/${id}/maxresdefault.jpg`),
    );
    return thumbUrls;
  };
  const [currentIndex, setCurrentIndex] = useState(0);
  const [currentVid, setCurrentVid] = useState(
    <LiteYouTubeEmbed id={ids[currentIndex]} title="Title" adNetwork={false} />,
  );
  useEffect(() => {
    setCurrentVid(
      <LiteYouTubeEmbed
        id={ids[currentIndex]}
        title="Title"
        adNetwork={false}
      />,
    );
  }, [currentIndex, ids]);
  const handlePrev = () => {
    if (currentIndex > 0) {
      setCurrentIndex(currentIndex - 1);
    }
  };
  const handleNext = () => {
    if (currentIndex < ids.length - 1) {
      setCurrentIndex(currentIndex + 1);
    }
  };

  return (
    <>
      <div className="">{currentVid}</div>
      <div className="flex flex-row items-center align-right bg-sui-ghost-white dark:bg-sui-ghost-dark rounded-lg mt-2">
        <div
          className={`flex items-center justify-center w-[200px] h-[100px] drop-shadow-none transition-[filter] ease-in-out duration-300 hover:drop-shadow-[0_0_4px_rgba(0,249,251,0.8)] ${currentIndex > 0 ? "cursor-pointer" : "opacity-10"}`}
          onClick={handlePrev}
        >
          <LeftChevron />
        </div>
        <CarouselThumbs
          images={thumbs(ids)}
          currentIndex={currentIndex}
          changeIndex={setCurrentIndex}
          thumbsClassName="w-[200px] h-[100px] object-contain mr-2 cursor-pointer"
          containerClassName="flex flex-row overflow-hidden mx-12"
        />
        <div
          className={`flex items-center justify-center w-[200px] h-[100px] drop-shadow-none transition-[filter] ease-in-out duration-300 hover:drop-shadow-[0_0_4px_rgba(0,249,251,0.8)] ${currentIndex < ids.length - 1 ? "cursor-pointer" : "opacity-10"}`}
          onClick={handleNext}
        >
          <RightChevron />
        </div>
      </div>
    </>
  );
}
