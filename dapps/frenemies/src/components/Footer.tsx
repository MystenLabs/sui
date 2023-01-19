// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SocialDiscord24, SocialTwitter24 } from "@mysten/icons";

export function Footer() {
  return (
    <footer className="fixed bottom-0 left-0 right-0 flex items-center px-8 py-4 leading-none bg-sui-light text-steel-darker text-xs flex-col sm:flex-row gap-2">
      <div className="flex-1">
        See{" "}
        <a
          href="https://mystenlabs.com/legal?content=terms"
          target="_blank"
          className="underline"
        >
          Terms of Service
        </a>{" "}
        and{" "}
        <a
          href="https://mystenlabs.com/legal?content=privacy"
          target="_blank"
          className="underline"
        >
          Privacy Policy
        </a>
        .
      </div>
      <div className="flex items-center gap-6">
        <div>
          Copyright © 2023,{" "}
          <a
            href="https://mystenlabs.com/"
            target="_blank"
            className="text-[#D40551]"
          >
            Mysten Labs, Inc.
          </a>
        </div>
        <a
          href="https://twitter.com/mysten_labs"
          className="text-xl"
          target="_blank"
          aria-label="Twitter"
        >
          <SocialTwitter24 />
        </a>
        <a
          href="https://discord.gg/Sui"
          className="text-xl"
          target="_blank"
          aria-label="Discord"
        >
          <SocialDiscord24 />
        </a>
      </div>
    </footer>
  );
}
