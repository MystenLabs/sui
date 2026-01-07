# Mysten Labs Shared Docusaurus Documentation Components

This repo houses the shared custom components, plugins, and scripts used across all Sui Stack documentation sites (Sui, Walrus, Seal, SuiNS, etc).

This repo is a work in progress and will continue to be updated, as some sites have not yet adopted the Docusaurus framework.

## Shared components

The shared components for all sites are:
```
shared/
├── components/
│   ├── Cards/
│   │   ├── index.tsx
│   │   └── styles.module.css
│   ├── ExampleImport/
│   │   ├── index.tsx
│   │   └── styles.module.css
│   ├── Glossary/
│   │   ├── GlossaryPage.tsx
│   │   ├── GlossaryProvider.tsx
│   │   ├── Term.tsx
│   │   └── term.module.css
│   ├── ImportContent/
│   │   ├── index.tsx
│   │   └── utils.js
│   ├── RelatedLink/
│   │   └── index.tsx
│   ├── SidebarIframe/
│   │   ├── index.js
│   │   └── styles.module.css
│   ├── Snippet/
│   │   └── index.tsx
│   ├── ThemeToggle/
│   │   └── index.tsx
│   └── UnsafeLink/
│       └── index.tsx
├── css/
│   └── details.css
├── js/
│   ├── convert-release-notes.js
│   ├── generate-import-context.js
│   ├── tabs-md.client.js
│   ├── update-cli-output.js
│   └── utils.js
├── plugins/
│   ├── descriptions/
│   │   └── index.js
│   ├── inject-code/
│   │   ├── index.js
│   │   └── stepLoader.js
│   ├── plausible/
│   │   ├── index.ts
│   │   └── client/
│   │       └── index.ts
│   ├── tabs-md-client/
│   │   └── index.mjs
│   └── remark-glossary.js
├── rehype/
│   ├── rehype-fix-anchor-urls.mjs
│   ├── rehype-raw-only.mjs
│   └── rehype-tabs.mjs
```

## Components that cannot be shared

Despite the sites using the same plugins and components for:

1. Visitor metrics (Plausible)
2. Cookbook AI
3. Algolia Search
4. Push Feedback

Each of these has a custom configuration for their own API keys. These components are
thus managed individually.

Additionally, all `src/theme` components are unique to each site to prevent conflicts
between the styling of each individual site and the Docusaurus theme swizzling process.

## Sui-specific components

Components unique to the Sui documentation are as follows:

- client/pushfeedback-toc.js
- css/custom.css
- css/fonts.css
- components/API
- components/BetaTag
- components/EffortBox
- components/GetStartedLink
- components/GraphqlBetaLink
- components/HomepageFeatures
- components/Protocol
- components/ProtocolConfig
- components/Search
- components/SidebarIframe
- components/Snippet
- components/ThemeToggle
- plugins/askcookbook
- plugins/betatag
- plugins/effort
- plugins/framework
- plugins/protocol
- utils/getopenrpcspecs.js
- utils/grpc-download.js
- utils/massagegraphql.js

## Walrus-specific components
