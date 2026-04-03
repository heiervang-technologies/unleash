const fs = require('fs');
const path = require('path');

const bundleDir = '/home/me/.local/share/mise/installs/node/25.1.0/lib/node_modules/@google/gemini-cli/bundle';
const files = fs.readdirSync(bundleDir).filter(f => f.startsWith('chunk-') && f.endsWith('.js'));

for (const file of files) {
    const filePath = path.join(bundleDir, file);
    let content = fs.readFileSync(filePath, 'utf-8');
    
    const targetOld = `  const filtered = curatedHistory.filter(msg => msg.parts && msg.parts.length > 0);
  const merged = [];
  for (const msg of filtered) {
    if (merged.length > 0 && merged[merged.length - 1].role === msg.role) {
      merged[merged.length - 1] = { ...merged[merged.length - 1], parts: [...merged[merged.length - 1].parts, ...msg.parts] };
    } else {
      merged.push({ ...msg, parts: [...msg.parts] });
    }
  }
  return merged;`;
    
    const targetNew = `
  const filtered = curatedHistory.filter(msg => msg.parts && msg.parts.length > 0);
  let merged = [];
  for (const msg of filtered) {
    if (merged.length > 0 && merged[merged.length - 1].role === msg.role) {
      merged[merged.length - 1] = { ...merged[merged.length - 1], parts: [...merged[merged.length - 1].parts, ...msg.parts] };
    } else {
      merged.push({ ...msg, parts: [...msg.parts] });
    }
  }
  while (merged.length > 0 && merged[0].role !== "user") {
    merged.shift();
  }
  return merged;
`;

    if (content.includes(targetOld)) {
        content = content.replace(targetOld, targetNew);
        fs.writeFileSync(filePath, content, 'utf-8');
        console.log('Patched', file);
    }
}
