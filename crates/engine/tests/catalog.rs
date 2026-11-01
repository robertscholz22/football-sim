//! Catalog coverage: every principle in `docs/tactics/principles.md` has a status, and every `v1` principle is
//! referenced (by its ID) somewhere in the engine source.

use std::path::{Path, PathBuf};

fn catalog() -> Vec<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/tactics/principles.md");
    let text = std::fs::read_to_string(&path).expect("read principles.md");
    let mut out = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        let Some(rest) = l.strip_prefix("### ") else { continue };
        let id = rest.split_whitespace().next().unwrap_or("").to_string();
        let status = lines
            .peek()
            .and_then(|s| s.strip_prefix("Status: "))
            .map(|s| s.split(" ·").next().unwrap_or("").trim().to_string())
            .unwrap_or_default();
        out.push((id, status));
    }
    out
}

fn sources(dir: &Path, out: &mut String) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push_str(&std::fs::read_to_string(&p).unwrap());
        }
    }
}

#[test]
fn every_principle_has_an_id_and_a_status() {
    let cat = catalog();
    assert!(cat.len() >= 60, "catalog too small: {}", cat.len());
    for (id, status) in &cat {
        let ok_id = id.split_once('-').is_some_and(|(a, n)| a.chars().all(|c| c.is_ascii_uppercase()) && n.parse::<u32>().is_ok());
        assert!(ok_id, "malformed ID {id}");
        let ok = status == "v1" || status == "metric" || (status.starts_with("deferred(M") && status.ends_with(')'));
        assert!(ok, "{id}: status '{status}' is not v1 / metric / deferred(Mx)");
    }
}

#[test]
fn every_v1_principle_is_referenced_in_code() {
    let mut src = String::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut src);
    let missing: Vec<String> =
        catalog().into_iter().filter(|(id, s)| s == "v1" && !src.contains(id.as_str())).map(|(id, _)| id).collect();
    assert!(missing.is_empty(), "v1 principles not referenced in crates/engine/src: {missing:?}");
}
