fn main() {
    if cfg!(target_os = "windows") {
        // Add MinGW to PATH for windres
        if let Ok(current) = std::env::var("PATH") {
            let mingw = r"C:\dev\tools\mingw\mingw64\bin";
            std::env::set_var("PATH", format!("{};{}", mingw, current));
        }
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: could not compile Windows resources: {}", e);
        }
    }
}
