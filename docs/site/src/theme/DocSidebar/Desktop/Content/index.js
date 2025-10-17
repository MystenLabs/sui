// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import Content from '@theme-original/DocSidebar/Desktop/Content';
import SidebarIframe from '@site/src/components/SidebarIframe';

export default function ContentWrapper(props) {
  return (
    <>
      <div style={{ padding: '0.5rem' }}>
        <SidebarIframe 
          url="https://cal.com/forms/08983b87-8001-4df6-896a-0d7b60acfd79"
          label="Book Office Hours"
          icon="🗳️"
        />
        <SidebarIframe 
          url="https://discord.gg/sui"
          label="Join Discord"
          icon="💬"
          openInNewTab={true}
        />
      </div>
      <Content {...props} />
    </>
  );
}