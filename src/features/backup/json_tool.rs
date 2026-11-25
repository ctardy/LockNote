// Developer tool, test builds only (never in the shipped exe): turn a full
// backup into readable JSON and back, to reorganise a note by hand.
//
//   LN_TOOL_MODE=dump  LN_TOOL_IN=<file.lnbak> LN_TOOL_OUT=<file.json>
//   LN_TOOL_MODE=pack  LN_TOOL_IN=<file.json>  LN_TOOL_OUT=<file.lnbak>
//   LN_TOOL_PW=<backup password>
//   cargo test features::backup::json_tool::run -- --ignored --exact --nocapture
//
// The JSON is the v1 body of the file (`to_json`), pretty-printed. It holds
// every secret in clear: delete it once packed. `pack` reads it with the
// import parser (`parse_v1`), encrypts it like an export, and refuses to write
// the file unless decoding it gives the same content back. An existing output
// file is never overwritten.

use super::*;
use std::path::Path;

pub(super) fn dump(input: &Path, output: &Path, password: &str) -> Result<String, String> {
    let bytes = std::fs::read(input).map_err(|e| format!("read {}: {}", input.display(), e))?;
    let backup = decode(&bytes, password).map_err(|e| format!("decode {}: {}", input.display(), e))?;
    let json = to_json(&backup, FORMAT);
    let value: Value = serde_json::from_str(&json).map_err(|e| format!("re-read JSON: {}", e))?;
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| format!("format JSON: {}", e))?;
    write_new(output, pretty.as_bytes())?;
    Ok(summary(&backup))
}

pub(super) fn pack(input: &Path, output: &Path, password: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(input).map_err(|e| format!("read {}: {}", input.display(), e))?;
    let backup = from_json(&text)?;
    let bytes = encode(&backup, password, DEFAULT_KDF).map_err(|e| format!("encode: {}", e))?;
    let back = decode(&bytes, password).map_err(|e| format!("decode of the packed file: {}", e))?;
    if *to_json(&back, FORMAT) != *to_json(&backup, FORMAT) {
        return Err("the packed file does not decode to the same content: nothing written".into());
    }
    write_new(output, &bytes)?;
    Ok(summary(&back))
}

fn from_json(text: &str) -> Result<Backup, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {}", e))?;
    let Value::Object(mut root) = root else {
        return Err("the JSON is not an object".into());
    };
    if root.get("format").and_then(Value::as_u64) != Some(u64::from(FORMAT)) {
        return Err(format!("\"format\" must be {}", FORMAT));
    }
    if root.get("kind").and_then(Value::as_str) != Some(KIND) {
        return Err(format!("\"kind\" must be \"{}\"", KIND));
    }
    parse_v1(&mut root).map_err(|e| format!("parse: {}", e))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {} (an existing file is never overwritten): {}", path.display(), e))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| format!("write {}: {}", path.display(), e))
}

fn summary(b: &Backup) -> String {
    format!(
        "tabs={} prompts={} secrets={} attachments={} snapshots={} custom_categories={}",
        b.tabs.len(),
        b.prompts.len(),
        b.secrets.len(),
        b.attachments.len(),
        b.snapshots.len(),
        b.settings.vault_categories.len()
    )
}

/// The tool itself, driven by the environment (see the top of the file).
#[test]
#[ignore]
fn run() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{} is not set", k));
    let (mode, input, output, password) = (var("LN_TOOL_MODE"), var("LN_TOOL_IN"), var("LN_TOOL_OUT"), var("LN_TOOL_PW"));
    let result = match mode.as_str() {
        "dump" => dump(Path::new(&input), Path::new(&output), &password),
        "pack" => pack(Path::new(&input), Path::new(&output), &password),
        other => Err(format!("LN_TOOL_MODE must be dump or pack, not {:?}", other)),
    };
    match result {
        Ok(s) => eprintln!("json_tool: {} ok in={} out={} {}", mode, input, output, s),
        Err(e) => panic!("json_tool: {} failed in={} out={}: {}", mode, input, output, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ln-json-tool-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> Backup {
        let mut settings = Settings::default();
        settings.vault_categories = vec!["Banque".to_string()];
        Backup {
            settings,
            tabs: vec![NoteTab::with_content("Accueil", "ligne 1\n\"quote\" \\ tab\t", Some("#AA3300"))],
            snapshots: Vec::new(),
            prompts: vec![crate::features::prompts::PromptEntry::new("Résumé", "Résume :\n\"cité\"\t", "Écriture")],
            secrets: vec![SecretEntry::in_category("Compte", "p@ss\nword", "Banque")],
            attachments: vec![Attachment::new("a.bin", vec![0, 1, 2, 255])],
            created_unix: 1_700_000_000,
            app_version: "1.2.24".into(),
        }
    }

    #[test]
    fn dump_then_pack_keeps_everything() {
        let d = dir("roundtrip");
        let src = d.join("in.lnbak");
        std::fs::write(&src, encode(&sample(), "pw", (1024, 1, 1)).unwrap()).unwrap();
        dump(&src, &d.join("x.json"), "pw").unwrap();
        let text = std::fs::read_to_string(d.join("x.json")).unwrap();
        assert!(text.contains("p@ss\\nword") && text.contains("\"Banque\""), "secrets and categories in clear");
        assert!(text.contains("\"prompts\"") && text.contains("\"Écriture\""), "prompts in clear");
        pack(&d.join("x.json"), &d.join("out.lnbak"), "pw").unwrap();
        let back = decode(&std::fs::read(d.join("out.lnbak")).unwrap(), "pw").unwrap();
        assert_eq!(*to_json(&back, FORMAT), *to_json(&sample(), FORMAT));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn edited_json_is_packed_with_the_edits() {
        let d = dir("edit");
        let src = d.join("in.lnbak");
        std::fs::write(&src, encode(&sample(), "pw", (1024, 1, 1)).unwrap()).unwrap();
        dump(&src, &d.join("x.json"), "pw").unwrap();
        let edited = std::fs::read_to_string(d.join("x.json")).unwrap().replace("\"Accueil\"", "\"Sommaire\"");
        std::fs::write(d.join("y.json"), edited).unwrap();
        pack(&d.join("y.json"), &d.join("out.lnbak"), "pw").unwrap();
        let back = decode(&std::fs::read(d.join("out.lnbak")).unwrap(), "pw").unwrap();
        assert_eq!(back.tabs[0].name, "Sommaire");
        assert_eq!(back.secrets[0].value, "p@ss\nword");
        assert_eq!(back.prompts, sample().prompts);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn refusals_are_explicit_and_write_nothing() {
        let d = dir("refuse");
        let src = d.join("in.lnbak");
        std::fs::write(&src, encode(&sample(), "pw", (1024, 1, 1)).unwrap()).unwrap();
        assert!(dump(&src, &d.join("x.json"), "wrong").unwrap_err().contains("decode"));
        assert!(!d.join("x.json").exists());
        dump(&src, &d.join("x.json"), "pw").unwrap();
        // existing output never overwritten
        assert!(dump(&src, &d.join("x.json"), "pw").unwrap_err().contains("never overwritten"));
        std::fs::write(d.join("bad.json"), "{\"format\":2,\"kind\":\"x\"}").unwrap();
        assert!(pack(&d.join("bad.json"), &d.join("o.lnbak"), "pw").unwrap_err().contains("format"));
        std::fs::write(d.join("bad2.json"), "not json").unwrap();
        assert!(pack(&d.join("bad2.json"), &d.join("o.lnbak"), "pw").unwrap_err().contains("invalid JSON"));
        assert!(pack(&d.join("x.json"), &d.join("o.lnbak"), "").unwrap_err().contains("empty password"));
        assert!(!d.join("o.lnbak").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
