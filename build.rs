fn main() {
    let profiles_dir = std::path::Path::new("src/profiles");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_file = std::path::Path::new(&out_dir).join("profile_list.rs");

    let mut entries: Vec<_> = std::fs::read_dir(profiles_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "txt").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut code = String::from("pub fn builtin_profiles_raw() -> Vec<(&'static str, &'static str)> {\n    vec![\n");
    for entry in &entries {
        let stem = entry.path().file_stem().unwrap().to_str().unwrap().to_string();
        code.push_str(&format!(
            "        ({:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/profiles/{}.txt\"))),\n",
            stem, stem
        ));
    }
    code.push_str("    ]\n}\n");
    std::fs::write(&out_file, code).unwrap();

    println!("cargo::rerun-if-changed=src/profiles/");
}
