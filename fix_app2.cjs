const fs = require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src/App.tsx', 'utf8');

// The file contains corrupted characters like éˆ and  and ?
// This is breaking JSX syntax, such as ?/span>
// We'll replace all non-ASCII characters with an empty string or 'TEXT'.
// Let's replace anything above 127 with '' and then fix obvious JSX issues.
b = b.replace(/[^\x00-\x7F]/g, '');

// Now we need to fix syntax where the quote or angle bracket was swallowed by the corrupted character.
// For example:
// {items.length} ?/span> -> {items.length} </span>
b = b.replace(/\?\/span>/g, '</span>');

// "message: ?} -> message: " "}
b = b.replace(/\?\}/g, '"}');
b = b.replace(/\?,/g, '",');
b = b.replace(/\?;/g, '";');
b = b.replace(/\?\)/g, '")');
b = b.replace(/\? : /g, '" : ');
b = b.replace(/\? \? /g, '" ? ');
b = b.replace(/\? \}/g, '" }');
b = b.replace(/="[^"]*\?\/span>/g, '="</span>');
b = b.replace(/return "\?;/g, 'return "";');
b = b.replace(/return "\?;/g, 'return "";');

fs.writeFileSync('C:/test/learn3/software-manager/src/App.tsx', b, 'utf8');
