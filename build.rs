fn main() {
    if cfg!(target_os = "windows") {
        // rc.exe of the Windows SDK (MSVC toolchain), or windres of a MinGW
        // found on PATH (GNU toolchain): winres picks the one of the target.
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "LockNote");
        res.set("FileDescription", "LockNote — Encrypted notepad for Windows");
        res.set("CompanyName", "uitguard.com");
        res.set("LegalCopyright", "\u{00A9} 2025-2026 Christophe Tardy — GPL-3.0-or-later");
        res.set("OriginalFilename", "LockNote.exe");
        res.set_manifest(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
      <supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"/>
    </application>
  </compatibility>
</assembly>
"#);
        // No silent fallback: an exe without its icon, version and manifest
        // (common controls 6, per-monitor DPI) must not be produced.
        if let Err(e) = res.compile() {
            panic!("could not compile Windows resources (windres or rc.exe): {}", e);
        }
    }
}
