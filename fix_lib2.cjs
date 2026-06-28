const fs=require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', 'utf8');

// The issue is some quotes were eaten by the corruption.
// Let's replace any .ok_or("...)? with .ok_or("error")?
b = b.replace(/\.ok_or\("[^\)]*\)\?;/g, '.ok_or("error")?;');

fs.writeFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', b, 'utf8');
