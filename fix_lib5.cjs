const fs=require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', 'utf8');
b = b.replace(/#\[tauri::command\]\nfn restart_as_admin_cmd\(_app/g, '#[cfg(not(windows))]\n#[tauri::command]\nfn restart_as_admin_cmd(_app');
fs.writeFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', b, 'utf8');
