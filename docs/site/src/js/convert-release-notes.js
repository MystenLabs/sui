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

function processLocalContent(content) {
  // Remove horizontal rules
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
        // Keep the first heading as H4
        processedLines.push(`#### ${text}`);
        firstHeadingFound = true;
      } else if (text.toLowerCase().includes('full log')) {
        // Make "Full log" headings H5
        processedLines.push(`##### ${text}`);
      } else {
        // All other headings become H5
        processedLines.push(`##### ${text}`);
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
  // Convert all headings to H3, except "Full log" which becomes H5
  // Also skip headings that are "Protocol" followed by protocol version info
  return content.replace(/^(#{1,6})\s+(.*)$/gm, (match, hashes, text) => {
    const trimmedText = text.trim();
    
    // Skip "Protocol" headings that appear to be protocol version info
    if (trimmedText.toLowerCase() === 'protocol') {
      return ''; // Remove this heading entirely
    }
    
    if (trimmedText.toLowerCase().includes('full log')) {
      return `##### ${trimmedText}`;
    } else {
      return `### ${trimmedText}`;
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
        
        content = content.replace(/^---\n[\s\S]*?\n---\n/, '');
        content = processLocalContent(content);
        
        localNotesByVersion.get(versionDir).push({
          fileName: file.replace('.md', ''),
          content: content.trim(),
          filePath: filePath
        });
      });
    });

    const totalLocalNotes = Array.from(localNotesByVersion.values()).reduce((sum, notes) => sum + notes.length, 0);
    console.log(`✓ Found ${totalLocalNotes} local release notes across ${localNotesByVersion.size} versions`);
  }

  const allReleaseNotes = [];
  
  try {
    console.log('Fetching GitHub releases...');
    const githubReleases = await fetchGitHubReleases();
    
    githubReleases.forEach(release => {
      if (release.draft) {
        return;
      }

      const headingName = release.tag_name || release.name;
      const versionKey = extractVersionFromTag(headingName);
      const network = extractNetwork(headingName);
      let githubContent = release.body || 'No release notes provided.';

      const localNotes = versionKey ? localNotesByVersion.get(versionKey) : null;

      let localContent = '';
      let processedGitHubContent = '';
      
      if (localNotes && localNotes.length > 0) {
        localContent = localNotes.map(note => note.content).join('\n\n');
        console.log(`✓ Merged ${localNotes.length} local note(s) from ${versionKey}/ with GitHub release ${headingName}`);
        localNotesByVersion.delete(versionKey);
      }
      
      processedGitHubContent = sanitizeForMDX(githubContent);
      processedGitHubContent = convertGitHubHeadingsToH3(processedGitHubContent);
      // Clean up any empty lines from removed headings
      processedGitHubContent = processedGitHubContent.replace(/\n{3,}/g, '\n\n');

      allReleaseNotes.push({
        heading: headingName,
        network: network,
        localContent: localContent,
        githubContent: processedGitHubContent,
        source: localContent ? 'combined' : 'github',
        date: release.published_at
      });
    });

    console.log(`✓ Found ${githubReleases.length} GitHub releases`);
  } catch (error) {
    console.error('⚠️ Error fetching GitHub releases:', error.message);
  }

  if (allReleaseNotes.length === 0) {
    console.log('No release notes found.');
    return;
  }

  allReleaseNotes.sort((a, b) => b.heading.localeCompare(a.heading));

  let consolidatedContent = `---
sidebar_position: 999
sidebar_label: 'Release Notes'
title: 'Release Notes'
hide_table_of_contents: true
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Release Notes

<Tabs groupId="network" queryString>
  <TabItem value="all" label="All Networks" default>
`;

  // All networks tab
  allReleaseNotes.forEach((note) => {
    consolidatedContent += `
## ${note.heading}

`;
    
    if (note.source === 'github') {
      consolidatedContent += `*Source: [GitHub Release](https://github.com/MystenLabs/sui/releases/tag/${note.heading})*\n\n`;
    } else if (note.source === 'combined') {
      consolidatedContent += `*Sources: Local release notes and [GitHub Release](https://github.com/MystenLabs/sui/releases/tag/${note.heading})*\n\n`;
    }
    
    // Add Protocol heading if there's local content
    if (note.localContent) {
      consolidatedContent += `### Protocol\n\n`;
      consolidatedContent += note.localContent + '\n\n';
    }
    
    consolidatedContent += note.githubContent + '\n\n';
    consolidatedContent += '---\n\n';
  });

  consolidatedContent += `
  </TabItem>
`;

  // Create tabs for each network
  ['mainnet', 'testnet', 'devnet'].forEach(network => {
    const networkReleases = allReleaseNotes.filter(note => note.network === network);
    
    if (networkReleases.length > 0) {
      const networkLabel = network.charAt(0).toUpperCase() + network.slice(1);
      consolidatedContent += `
  <TabItem value="${network}" label="${networkLabel}">
`;

      networkReleases.forEach((note) => {
        consolidatedContent += `
## ${note.heading}

`;
        
        if (note.source === 'github') {
          consolidatedContent += `*Source: [GitHub Release](https://github.com/MystenLabs/sui/releases/tag/${note.heading})*\n\n`;
        } else if (note.source === 'combined') {
          consolidatedContent += `*Sources: Local release notes and [GitHub Release](https://github.com/MystenLabs/sui/releases/tag/${note.heading})*\n\n`;
        }
        
        // Add Protocol heading if there's local content
        if (note.localContent) {
          consolidatedContent += `### Protocol\n\n`;
          consolidatedContent += note.localContent + '\n\n';
        }
        
        consolidatedContent += note.githubContent + '\n\n';
        consolidatedContent += '---\n\n';
      });

      consolidatedContent += `
  </TabItem>
`;
    }
  });

  consolidatedContent += `
</Tabs>
`;

  const outputDir = path.dirname(outputReleaseNotesPath);
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  fs.writeFileSync(outputReleaseNotesPath, consolidatedContent, 'utf8');
  console.log(`✓ Consolidated ${allReleaseNotes.length} total release notes into: ${outputReleaseNotesPath}`);
}

convertMdToMdx(docsDir);
consolidateReleaseNotes().catch(error => {
  console.error('Error:', error);
  process.exit(1);
});

console.log('✓ MDX generation complete!');
