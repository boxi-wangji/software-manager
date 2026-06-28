const fs=require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', 'utf8');
b = b.replace(/\/\/ 挑出便携\?    let portable = release/g, 'let portable = release');
fs.writeFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', b, 'utf8');
