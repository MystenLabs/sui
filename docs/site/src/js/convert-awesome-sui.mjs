// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

// Get __dirname equivalent in ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Paths (adjusted for new location)
const readmePath = path.join(
  __dirname,
  "../../../submodules/awesome-sui/README.md",
);
const detailsSourceDir = path.join(
  __dirname,
  "../../../submodules/awesome-sui/details",
);
const mediaSourceDir = path.join(
  __dirname,
  "../../../submodules/awesome-sui/media",
);
const readmeTargetPath = path.join(
  __dirname,
  "../../../content/references/awesome-sui.mdx",
);
const detailsTargetDir = path.join(
  __dirname,
  "../../../content/references/awesome-sui",
);
const mediaTargetDir = path.join(
  __dirname,
  "../../static/awesome-sui/media",
);

// Process the content to remove the Contents section and transform list items
function processContent(content) {
  // Skip initial content: level 1 heading, anchor element, and first line starting with '>'
  // Find the first paragraph (starts after the first '>' line)
  const skipInitialRegex = /^# [^\n]*\n\n<a [^>]*>.*?<\/a>\n\n> [^\n]*\n\n/;
  let processedContent = content.replace(skipInitialRegex, "");

  // Find the "## Contents" section and remove it along with its list
  const contentsRegex = /## Contents\n\n(?:- .*\n(?: {2}- .*\n)*)*\n/;
  processedContent = processedContent.replace(contentsRegex, "");

  // Fix CONTRIBUTING.md link
  processedContent = processedContent.replace(
    /\[([^\]]*)\]\(CONTRIBUTING\.md\)/g,
    "[$1](https://github.com/sui-foundation/awesome-sui/blob/main/CONTRIBUTING.md)",
  );

  // Change details/*.md links to awesome-sui/*.mdx and replace dashes with underscores as a temporary fix for a broken link in readme
  processedContent = processedContent.replace(
    /\[([^\]]*)\]\(details\/([^)]+)\.md\)/g,
    (match, linkText, filename) => `[${linkText}](./awesome-sui/${filename.replace(/-/g, '_')}.mdx)`,
  );

  // Change media/* links to /awesome-sui/media/* (served from static folder)
  processedContent = processedContent.replace(
    /media\/([^")\s]+)/g,
    "/awesome-sui/media/$1",
  );

  // Convert #### headings to ### headings
  processedContent = processedContent.replace(/^#### /gm, "### ");

  // Transform top-level list items - main text becomes heading, content after " - " becomes paragraph
  processedContent = processedContent.replace(
    /^- (.+?)( - .+)?$/gm,
    (match, mainText, dashSeparatedItems) => {
      // Start outer wrapper div
      let result = `<div className="border border-solid border-sui-gray-50 rounded-lg my-4">\n\n`;
      
      // Add heading div with p-4, and h4 with mb-0
      result += `<div className="bg-sui-gray-50 p-4 rounded-t">\n\n<h4 className="mb-0">${mainText.trim()}</h4>\n\n</div>`;

      if (dashSeparatedItems) {
        // Content after " - " becomes a paragraph in content div
        const paragraphContent = dashSeparatedItems.substring(3).trim(); // Remove the " - " prefix
        result += `\n\n<div className="p-4">\n\n${paragraphContent}`;
      }

      return result;
    },
  );

  // Handle nested list items (items that start with "  -")
  processedContent = processedContent.replace(
    /^ {2}- (.+)$/gm,
    (match, item) => {
      // Check if this item contains " - " separators
      if (item.includes(" - ")) {
        const parts = item.split(" - ").map((part) => `- ${part.trim()}`);
        return parts.join("\n");
      }
      return `- ${item}`;
    },
  );

  // Add closing div tags
  const lines = processedContent.split("\n");
  const result = [];
  let insideDiv = false;
  let hasContentDiv = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Check if this line starts a new div section
    if (
      line.startsWith(
        '<div className="border border-solid border-sui-gray-50 rounded-lg my-4">',
      )
    ) {
      if (insideDiv) {
        // Close previous divs
        if (hasContentDiv) {
          result.push("</div>");
          result.push("");
        }
        result.push("</div>");
        result.push("");
      }
      result.push(line);
      insideDiv = true;
      hasContentDiv = false;
    }
    // Check if this line starts a content div
    else if (insideDiv && line.startsWith('<div className="p-4">')) {
      result.push(line);
      hasContentDiv = true;
    }
    // Check if we need to close a div (before level 2-3 headings)
    else if (insideDiv && line.match(/^#{2,3} /)) {
      // Close divs
      if (hasContentDiv) {
        result.push("");
        result.push("</div>");
      }
      result.push("");
      result.push("</div>");
      result.push("");
      result.push(line); // Add the heading that triggered the close
      insideDiv = false;
      hasContentDiv = false;
    }
    // Regular line inside a div
    else if (insideDiv) {
      result.push(line);
    }
    // Regular line outside div
    else {
      result.push(line);
    }
  }

  // Close any remaining open divs
  if (insideDiv) {
    if (hasContentDiv) {
      result.push("");
      result.push("</div>");
    }
    result.push("");
    result.push("</div>");
  }

  return result.join("\n");
}

// Generate MDX frontmatter for detail files
function generateDetailFrontmatter(filename) {
  // Convert filename to title (e.g., "sdk_sui_typescript.md" -> "SDK Sui TypeScript")
  const title = filename
    .replace(/\.md$/, "")
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");

  return `---
title: ${title}
description: ${title} details from Awesome Sui.
---

`;
}

// Process detail file content to remove level-1 headings
function processDetailContent(content) {
  // Remove the first level-1 heading if it exists
  return content.replace(/^# [^\n]*\n\n?/, "");
}

// Convert README.md
console.log("Reading README file:", readmePath);
const readmeContent = fs.readFileSync(readmePath, "utf8");
const processedReadmeContent = processContent(readmeContent);

const readmeMdxContent = `---
title: Awesome Sui
description: A curated list of awesome developer tools and infrastructure projects within the Sui ecosystem
---

:::info

Visit the [Awesome Sui repo](https://github.com/sui-foundation/awesome-sui/tree/main) on GitHub for the source content for these pages.

:::

${processedReadmeContent}`;

// Ensure target directories exist
const readmeTargetDir = path.dirname(readmeTargetPath);
if (!fs.existsSync(readmeTargetDir)) {
  fs.mkdirSync(readmeTargetDir, { recursive: true });
}

if (!fs.existsSync(detailsTargetDir)) {
  fs.mkdirSync(detailsTargetDir, { recursive: true });
}

if (!fs.existsSync(mediaTargetDir)) {
  fs.mkdirSync(mediaTargetDir, { recursive: true });
}

// Write the main README MDX file
console.log("Writing README target file:", readmeTargetPath);
fs.writeFileSync(readmeTargetPath, readmeMdxContent, "utf8");

// Process all MD files in the details folder
if (fs.existsSync(detailsSourceDir)) {
  const detailFiles = fs
    .readdirSync(detailsSourceDir)
    .filter((file) => file.endsWith(".md"));

  console.log(`Processing ${detailFiles.length} detail files...`);

  detailFiles.forEach((filename) => {
    const sourcePath = path.join(detailsSourceDir, filename);
    const targetPath = path.join(
      detailsTargetDir,
      filename.replace(".md", ".mdx"),
    );

    console.log(`Converting ${filename} -> ${path.basename(targetPath)}`);

    const content = fs.readFileSync(sourcePath, "utf8");
    const processedContent = processDetailContent(content);
    const frontmatter = generateDetailFrontmatter(filename);
    const mdxContent = frontmatter + processedContent;

    fs.writeFileSync(targetPath, mdxContent, "utf8");
  });

  console.log(
    `✅ Successfully converted README.md and ${detailFiles.length} detail files`,
  );
} else {
  console.log("⚠️ Details folder not found, only converted README.md");
}

// Copy media files to static folder
if (fs.existsSync(mediaSourceDir)) {
  // Ensure static media target directory exists
  if (!fs.existsSync(mediaTargetDir)) {
    fs.mkdirSync(mediaTargetDir, { recursive: true });
  }

  const mediaFiles = fs.readdirSync(mediaSourceDir);

  console.log(`Copying ${mediaFiles.length} media files...`);

  mediaFiles.forEach((filename) => {
    const sourcePath = path.join(mediaSourceDir, filename);
    const targetPath = path.join(mediaTargetDir, filename);

    console.log(`Copying ${filename}`);

    fs.copyFileSync(sourcePath, targetPath);
  });

  console.log(`✅ Successfully copied ${mediaFiles.length} media files`);
} else {
  console.log("⚠️ Media folder not found");
}
