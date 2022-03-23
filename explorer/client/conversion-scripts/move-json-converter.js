const fs = require('fs');

const contents = fs.readFileSync('./Example.move', { encoding: 'utf8' });

const data = JSON.stringify({ data: contents });

fs.writeFileSync('moveExample.json', data);
