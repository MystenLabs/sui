const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

console.log('🚀 Starting CLI documentation update...');
console.log('Current working directory:', process.cwd());

function updateCliOutput() {
  try {
    console.log('📋 Running CLI command...');
    
    // Replace 'your-cli-command --help' with your actual command
    const command = 'sui client --help'; // Start with a simple command that definitely works
    console.log('Command:', command);
    
    const output = execSync(command, { 
      encoding: 'utf8',
      timeout: 10000
    });
    
    console.log('✅ Command executed successfully');
    console.log('Output length:', output.length);
    console.log('First 100 chars:', output.substring(0, 100));

    // Update this path to your actual MDX file
    const mdxFile = path.join(process.cwd(), '..', 'content', 'references', 'cli', 'client.mdx');
    console.log('Target file:', mdxFile);
    
    if (!fs.existsSync(mdxFile)) {
      console.error('❌ MDX file not found:', mdxFile);
      console.log('Available files in docs/:');
      const docsDir = path.join(process.cwd(), 'docs');
      if (fs.existsSync(docsDir)) {
        fs.readdirSync(docsDir).forEach(file => {
          console.log('  -', file);
        });
      } else {
        console.log('❌ docs/ directory not found');
      }
      return;
    }

    let content = fs.readFileSync(mdxFile, 'utf8');
    console.log('📄 Original file length:', content.length);

    const startMarker = '<!-- CLI_OUTPUT_START -->';
    const endMarker = '<!-- CLI_OUTPUT_END -->';
    
    console.log('🔍 Looking for markers...');
    console.log('Start marker found:', content.includes(startMarker));
    console.log('End marker found:', content.includes(endMarker));
    
    if (!content.includes(startMarker) || !content.includes(endMarker)) {
      console.error('❌ Markers not found in file. File content preview:');
      console.log(content.substring(0, 500));
      return;
    }

    const regex = new RegExp(`${startMarker}[\\s\\S]*?${endMarker}`, 'g');
    const replacement = `${startMarker}
\`\`\`bash
${output.trim()}
\`\`\`
${endMarker}`;

    const newContent = content.replace(regex, replacement);
    
    if (newContent === content) {
      console.log('⚠️ No changes made to content');
    } else {
      console.log('✏️ Content will be updated');
      fs.writeFileSync(mdxFile, newContent);
      console.log('✅ File written successfully');
    }

  } catch (error) {
    console.error('❌ Error details:', error);
  }
}

updateCliOutput();