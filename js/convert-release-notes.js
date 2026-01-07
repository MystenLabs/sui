/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

const fs = require('fs');
const path = require('path');
const https = require('https');

const docsDir = '../../docs';
const releaseNotesDir = '../../release-notes/';
const outputReleaseNotesPath = '../../docs/content/references/release-notes.mdx';

const excludeDirs = ['node_modules', '.git', 'build', 'dist', '.docusaurus'];

function shouldExcludeDir(dirName) {
  return excludeDirs.some(excluded => dirName.includes(excluded));
}

function convertMdToMdx(dirPath) {
  if (shouldExcludeDir(dirPath)) {
    return;
  }

  const files = fs.readdirSync(dirPath);

  files.forEach(file => {
    const filePath = path.join(dirPath, file);
    
    if (shouldExcludeDir(filePath)) {
      return;
    }

    const stat = fs.statSync(filePath);

    if (stat.isDirectory()) {
      convertMdToMdx(filePath);
    } else if (file.endsWith('.md') && !file.endsWith('.mdx')) {
      const mdxPath = filePath.replace(/\.md$/, '.mdx');
      const content = fs.readFileSync(filePath, 'utf8');
      
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

function extractVersionFromTag(tag) {
  const match = tag.match(/v?(\d+)\.(\d+)(?:\.\d+)?/i);
  if (match) {
    return `${match[1]}_${match[2]}`;
  }
  return null;
}

function extractNetwork(tag) {
  const lower = tag.toLowerCase();
  if (lower.includes('testnet')) return 'testnet';
  if (lower.includes('devnet')) return 'devnet';
  if (lower.includes('mainnet')) return 'mainnet';
  return 'other';
}

function removeNetworkPrefix(tag) {
  // Remove mainnet-, testnet-, devnet- prefixes
  return tag.replace(/^(mainnet|testnet|devnet)-/i, '');
}

function parseVersion(tag) {
  const match = tag.match(/v?(\d+)\.(\d+)\.(\d+)/i);
  if (!match) return null;
  
  return {
    major: parseInt(match[1]),
    minor: parseInt(match[2]),
    patch: parseInt(match[3]),
    original: tag
  };
}

function getVersionKey(version) {
  return `${version.major}.${version.minor}.${version.patch}`;
}

function compareVersions(v1, v2) {
  if (v1.major !== v2.major) return v1.major - v2.major;
  if (v1.minor !== v2.minor) return v1.minor - v2.minor;
  return v1.patch - v2.patch;
}

function extractFirstHeading(content) {
  // Extract the first heading from the content
  const lines = content.split('\n');
  for (let line of lines) {
    const headingMatch = line.match(/^#{1,6}\s+(.+)$/);
    if (headingMatch) {
      return headingMatch[1].trim();
    }
  }
  return null;
}

function processLocalContent(content) {
  content = content.replace(/^---+\s*$/gm, '');
  content = content.replace(/\n{3,}/g, '\n\n');
  
  const lines = content.split('\n');
  const processedLines = [];
  let firstHeadingFound = false;
  
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const headingMatch = line.match(/^(#{1,6})\s+(.*)$/);
    
    if (headingMatch) {
      const text = headingMatch[2].trim();
      
      if (!firstHeadingFound) {
        // Skip the first heading - we'll use it as the summary
        firstHeadingFound = true;
        continue;
      } else if (text.toLowerCase().includes('full log')) {
        processedLines.push(`#### ${text}`);
      } else {
        processedLines.push(`#### ${text}`);
      }
    } else {
      processedLines.push(line);
    }
  }
  
  return processedLines.join('\n').trim();
}

function sanitizeForMDX(content) {
  content = content.replace(/(?<!\[#\d+\]\()https:\/\/github\.com\/([^/\s]+)\/([^/\s]+)\/pull\/(\d+)(?!\))/g, 
    '[#$3](https://github.com/$1/$2/pull/$3)');
  
  content = content.replace(/<([^>\s]+@[^>]+)>/g, '&lt;$1&gt;');
  content = content.replace(/<(\w+)@([\w.-]+)>/g, '&lt;$1@$2&gt;');
  
  const codeBlocks = [];
  content = content.replace(/(```[\s\S]*?```|`[^`]+`)/g, (match) => {
    codeBlocks.push(match);
    return `__CODE_BLOCK_${codeBlocks.length - 1}__`;
  });
  
  content = content.replace(/(\s|^)<(\s)/g, '$1&lt;$2');
  content = content.replace(/(\s)>(\s|$)/g, '$1&gt;$2');
  
  codeBlocks.forEach((block, index) => {
    content = content.replace(`__CODE_BLOCK_${index}__`, block);
  });
  
  return content;
}

function convertGitHubHeadingsToH3(content) {
  return content.replace(/^(#{1,6})\s+(.*)$/gm, (match, hashes, text) => {
    const trimmedText = text.trim();
    
    if (trimmedText.toLowerCase() === 'protocol') {
      return '';
    }
    
    if (trimmedText.toLowerCase().includes('full log')) {
      return `##### ${trimmedText}`;
    } else {
      return `#### ${trimmedText}`;
    }
  });
}

function fetchGitHubReleases() {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: 'api.github.com',
      path: '/repos/MystenLabs/sui/releases?per_page=100',
      method: 'GET',
      headers: {
        'User-Agent': 'Node.js Script',
        'Accept': 'application/vnd.github.v3+json'
      }
    };

    const githubToken = process.env.GITHUB_TOKEN;
    if (githubToken) {
      options.headers['Authorization'] = `token ${githubToken}`;
    }

    const req = https.request(options, (res) => {
      let data = '';

      res.on('data', (chunk) => {
        data += chunk;
      });

      res.on('end', () => {
        if (res.statusCode === 200) {
          resolve(JSON.parse(data));
        } else {
          reject(new Error(`GitHub API returned status ${res.statusCode}: ${data}`));
        }
      });
    });

    req.on('error', (error) => {
      reject(error);
    });

    req.end();
  });
}

async function consolidateReleaseNotes() {
  const localNotesByVersion = new Map();

  if (fs.existsSync(releaseNotesDir)) {
    const items = fs.readdirSync(releaseNotesDir);
    
    const versionDirs = items.filter(item => {
      const itemPath = path.join(releaseNotesDir, item);
      return fs.statSync(itemPath).isDirectory() && item.match(/^\d+_\d+$/);
    });

    versionDirs.forEach(versionDir => {
      const versionDirPath = path.join(releaseNotesDir, versionDir);
      const files = fs.readdirSync(versionDirPath)
        .filter(file => file.endsWith('.md') && file.toLowerCase() !== 'readme.md');
      
      if (!localNotesByVersion.has(versionDir)) {
        localNotesByVersion.set(versionDir, []);
      }
      
      files.forEach(file => {
        const filePath = path.join(versionDirPath, file);
        let content = fs.readFileSync(filePath, 'utf8');
        
        // Remove frontmatter if it exists
        content = content.replace(/^---\n[\s\S]*?\n---\n/, '');
        
        // Extract first heading before processing
        const firstHeading = extractFirstHeading(content);
        
        // Process content (this will remove the first heading)
        const processedContent = processLocalContent(content);
        
        localNotesByVersion.get(versionDir).push({
          fileName: file.replace('.md', ''),
          title: firstHeading || file.replace('.md', '').replace(/_/g, ' '),
          content: processedContent.trim(),
          filePath: filePath
        });
      });
    });

    const totalLocalNotes = Array.from(localNotesByVersion.values()).reduce((sum, notes) => sum + notes.length, 0);
    console.log(`✓ Found ${totalLocalNotes} local release notes across ${localNotesByVersion.size} versions`);
  }

  try {
    console.log('Fetching GitHub releases...');
    const githubReleases = await fetchGitHubReleases();
    
    // Group releases by version number
    const releasesByVersion = new Map();
    
    githubReleases.forEach(release => {
      if (release.draft) {
        return;
      }

      const tag = release.tag_name || release.name;
      const network = extractNetwork(tag);
      const version = parseVersion(tag);
      
      if (!version || (network !== 'mainnet' && network !== 'testnet')) {
        return;
      }
      
      const versionKey = getVersionKey(version);
      
      if (!releasesByVersion.has(versionKey)) {
        releasesByVersion.set(versionKey, {
          mainnet: null,
          testnet: null,
          version: version
        });
      }
      
      releasesByVersion.get(versionKey)[network] = {
        tag: tag,
        content: release.body || 'No release notes provided.',
        date: release.published_at
      };
    });
    
    // Convert to array and sort by version (newest first)
    const sortedReleases = Array.from(releasesByVersion.entries())
      .sort((a, b) => compareVersions(b[1].version, a[1].version));
    
    // Determine which is the latest version overall
    const latestVersion = sortedReleases.length > 0 ? sortedReleases[0][0] : null;
    
    // Build release notes
    const allReleaseNotes = [];
    
    sortedReleases.forEach(([versionKey, data]) => {
      const isLatest = versionKey === latestVersion;
      
      // For latest version, prefer testnet if available, otherwise mainnet
      // For older versions, always use mainnet
      let networkToUse;
      let releaseToUse;
      
      if (isLatest) {
        // Latest version - prefer testnet
        if (data.testnet) {
          networkToUse = 'testnet';
          releaseToUse = data.testnet;
        } else if (data.mainnet) {
          networkToUse = 'mainnet';
          releaseToUse = data.mainnet;
        } else {
          return; // Skip if no release
        }
      } else {
        // Older version - always use mainnet
        if (data.mainnet) {
          networkToUse = 'mainnet';
          releaseToUse = data.mainnet;
        } else {
          return; // Skip if no mainnet release
        }
      }
      
      // Get local notes
      const versionForLocal = extractVersionFromTag(releaseToUse.tag);
      const localNotes = versionForLocal ? localNotesByVersion.get(versionForLocal) : null;
      let localNotesData = [];
      
      if (localNotes && localNotes.length > 0) {
        localNotesData = localNotes.map(note => ({
          title: note.title,
          content: note.content
        }));
        localNotesByVersion.delete(versionForLocal);
      }
      
      // Process content
      let processedGitHubContent = sanitizeForMDX(releaseToUse.content);
      processedGitHubContent = convertGitHubHeadingsToH3(processedGitHubContent);
      processedGitHubContent = processedGitHubContent.replace(/\n{3,}/g, '\n\n');

      allReleaseNotes.push({
        version: versionKey,
        tag: releaseToUse.tag,
        displayTag: removeNetworkPrefix(releaseToUse.tag),
        network: networkToUse,
        localNotes: localNotesData,
        githubContent: processedGitHubContent,
        hasLocalContent: localNotesData.length > 0,
        isLatest: isLatest,
        date: releaseToUse.date
      });
    });

    let consolidatedContent = `---
sidebar_position: 999
sidebar_label: 'Release Notes'
title: 'Release Notes'
---

# Release Notes

---

`;

    // Generate content
    allReleaseNotes.forEach((note) => {
      consolidatedContent += `
## ${note.displayTag}

`;
      
      // Add network badge
      const networkBadge = note.network === 'testnet' ? '🔶 Testnet' : '✅ Mainnet';
      consolidatedContent += `**${networkBadge}** | *Source: [GitHub Release](https://github.com/MystenLabs/sui/releases/tag/${note.tag})*\n\n`;
      
      // Add local content in collapsible details if it exists
      if (note.localNotes.length > 0) {
        note.localNotes.forEach(localNote => {
          consolidatedContent += `<details>
<summary>${localNote.title}</summary>

${localNote.content}

</details>

`;
        });
      }
      
      consolidatedContent += note.githubContent + '\n\n';
      consolidatedContent += '---\n\n';
    });

    const outputDir = path.dirname(outputReleaseNotesPath);
    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true });
    }

    fs.writeFileSync(outputReleaseNotesPath, consolidatedContent, 'utf8');
    console.log(`✓ Consolidated ${allReleaseNotes.length} release notes into: ${outputReleaseNotesPath}`);
  } catch (error) {
    console.error('⚠️ Error fetching GitHub releases:', error.message);
  }
}

convertMdToMdx(docsDir);
consolidateReleaseNotes().catch(error => {
  console.error('Error:', error);
  process.exit(1);
});

console.log('✓ MDX generation complete!');
