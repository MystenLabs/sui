// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Social component
 * Use: Used as a base component to build any kind of social
 * Needs an icon (optional), target link (required), text (optional) and order (defaults|optional)
 */

 const Social = ({ icon, link, text, revert = false }) => {
    return (
      <a
        href={link}
        rel="noreferrer"
        className={`flex group items-center ${revert ? "flex-row-reverse" : ""}`}
        target="_blank"
      >
        {!!icon && (
          <span
            className={`flex w-[18px] h-[18px] text-inherit text-sui-sky/90 group-hover:text-sui-sky/80 ${
              revert ? "ml-2" : "mr-2"
            }`}
          >
            {icon()}
          </span>
        )}
  
        {!!text && (
          <span
            className={`text-inherit group-hover:text-sui-sky/50 ${
              revert ? "ml-2" : "mr-4"
            }`}
          >
            {text}
          </span>
        )}
      </a>
    );
  };
  
  export default Social;