const fs = require('fs');
const path = require('path');

const docsDir = '../../docs'; // Your docs directory
const releaseNotesDir = '../../release-notes/'; // Release notes directory
const outputReleaseNotesPath = '../../docs/content/references/release-notes.mdx'; // Output consolidated file

// Directories to exclude from processing
const excludeDirs = ['node_modules', '.git', 'build', 'dist', '.docusaurus'];

function shouldExcludeDir(dirName) {
  return excludeDirs.some(excluded => dirName.includes(excluded));
}

function convertMdToMdx(dirPath) {
  // Skip excluded directories
  if (shouldExcludeDir(dirPath)) {
    return;
  }

  const files = fs.readdirSync(dirPath);

  files.forEach(file => {
    const filePath = path.join(dirPath, file);
    
    // Skip excluded directories
    if (shouldExcludeDir(filePath)) {
      return;
    }

    const stat = fs.statSync(filePath);

    if (stat.isDirectory()) {
      convertMdToMdx(filePath); // Recursively process subdirectories
    } else if (file.endsWith('.md') && !file.endsWith('.mdx')) {
      const mdxPath = filePath.replace(/\.md$/, '.mdx');
      const content = fs.readFileSync(filePath, 'utf8');
      
      // Add frontmatter if it doesn't exist
      let mdxContent = content;
      if (!content.startsWith('---')) {
        const title = file.replace('.md', '').replace(/-/g, ' ');
        mdxContent = `---
sidebar_position: 1
---

${content}`;
      }
      
      fs.writeFileSync(mdxPath, mdxContent, 'utf8');
      console.log(`Converted: ${filePath} -> ${mdxPath}`);
    }
  });
}

function consolidateReleaseNotes() {
  if (!fs.existsSync(releaseNotesDir)) {
    console.log('Release notes directory not found, skipping...');
    return;
  }

  // Get all items in the release notes directory
  const items = fs.readdirSync(releaseNotesDir);
  
  // Filter for subdirectories only
  const subdirs = items.filter(item => {
    const itemPath = path.join(releaseNotesDir, item);
    return fs.statSync(itemPath).isDirectory();
  });

  if (subdirs.length === 0) {
    console.log('No release notes subdirectories found.');
    return;
  }

  // Collect all .md files from subdirectories
  const allFiles = [];
  subdirs.forEach(subdir => {
    const subdirPath = path.join(releaseNotesDir, subdir);
    const files = fs.readdirSync(subdirPath)
      .filter(file => file.endsWith('.md') && file.toLowerCase() !== 'readme.md')
      .map(file => ({
        path: path.join(subdirPath, file),
        name: file,
        subdir: subdir
      }));
    allFiles.push(...files);
  });

  if (allFiles.length === 0) {
    console.log('No release notes markdown files found in subdirectories.');
    return;
  }

  // Sort files (most recent first)
  allFiles.sort((a, b) => b.name.localeCompare(a.name));

  let consolidatedContent = `---
sidebar_position: 999
sidebar_label: 'Release Notes'
title: 'Release Notes'
---

# Release Notes

`;

  allFiles.forEach(fileInfo => {
    let content = fs.readFileSync(fileInfo.path, 'utf8');
    
    // Remove frontmatter if it exists
    content = content.replace(/^---\n[\s\S]*?\n---\n/, '');
    
    // Extract version/date from filename
    const fileName = fileInfo.name.replace('.md', '');
    
    // Add separator and content
    consolidatedContent += `\n---\n\n`;
    
    // If content doesn't start with a heading, add one from filename
    if (!content.trim().startsWith('#')) {
      consolidatedContent += `## ${fileName.replace(/-/g, ' ')}\n\n`;
    }
    
    consolidatedContent += content.trim() + '\n\n';
  });

  // Convert all H2s to H3s except the first one
  consolidatedContent = convertH2ToH3ExceptFirst(consolidatedContent);

  // Ensure output directory exists
  const outputDir = path.dirname(outputReleaseNotesPath);
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  fs.writeFileSync(outputReleaseNotesPath, consolidatedContent, 'utf8');
  console.log(`✓ Consolidated ${allFiles.length} release notes from ${subdirs.length} subdirectories into: ${outputReleaseNotesPath}`);
}

function convertH2ToH3ExceptFirst(content) {
  let firstH2Found = false;
  
  // Split content into lines
  const lines = content.split('\n');
  const processedLines = lines.map(line => {
    // Check if line is an H2 heading
    if (line.match(/^## /)) {
      if (!firstH2Found) {
        // Keep the first H2 as is
        firstH2Found = true;
        return line;
      } else {
        // Convert subsequent H2s to H3s
        return line.replace(/^## /, '### ');
      }
    }
    return line;
  });
  
  return processedLines.join('\n');
}

// Run both conversions
convertMdToMdx(docsDir);
consolidateReleaseNotes();

console.log('✓ MDX generation complete!');