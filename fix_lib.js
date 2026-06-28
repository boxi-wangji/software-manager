const fs=require('fs');
let b = fs.readFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', 'utf8');
b = b.replace(/\/\/.*æ£€æµ.*/g, '// check architecture');
b = b.replace(/\.ok_or\("[^"]*WeGame[^"]*"\)\?;/g, '.ok_or("WeGame Error")?;');
b = b.replace(/let file_name = file_name_from_url\(&url\)\.ok_or\("[^"]*"\)\?;/g, 'let file_name = file_name_from_url(&url).ok_or("error")?;');
b = b.replace(/let file_name = file_name_from_url\(&release\.download_url\)\.ok_or\("[^"]*"\)\?;/g, 'let file_name = file_name_from_url(&release.download_url).ok_or("error")?;');
b = b.replace(/let file_name = file_name_from_url\(&release\.download_url\)\.ok_or\("[^"]*\?\)\?;/g, 'let file_name = file_name_from_url(&release.download_url).ok_or("error")?;');
fs.writeFileSync('C:/test/learn3/software-manager/src-tauri/src/lib.rs', b, 'utf8');
