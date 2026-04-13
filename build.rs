fn main() {
    if cfg!(target_os = "windows") {
        // Add MinGW to PATH for windres
        if let Ok(current) = std::env::var("PATH") {
            let mingw = r"C:\dev\tools\mingw\mingw64\bin";
            std::env::set_var("PATH", format!("{};{}", mingw, current));
        }
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "LockNote");
        res.set("FileDescription", "LockNote — Encrypted notepad for Windows");
        res.set("CompanyName", "uitguard.com");
        res.set("LegalCopyright", "\u{00A9} 2026 Chris Tardy — uitguard.com");
        res.set("OriginalFilename", "LockNote.exe");
        if let Err(e) = res.compile() {
            eprintln!("Warning: could not compile Windows resources: {}", e);
        }
    }
}
