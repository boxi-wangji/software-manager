const fs=require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src/App.tsx', 'utf8');

// fix ? followed by punctuation
b = b.replace(/\?,/g, '",');
b = b.replace(/\?;/g, '";');
b = b.replace(/\?\)/g, '")');
b = b.replace(/\? \}/g, '" }');
b = b.replace(/\?}/g, '"}');
b = b.replace(/\? : /g, '" : ');
b = b.replace(/\? \? /g, '" ? ');

fs.writeFileSync('C:/test/learn3/software-manager/src/App.tsx', b, 'utf8');
