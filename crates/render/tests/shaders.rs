use std::fs;
use std::path::Path;

#[test]
fn all_wgsl_shaders_validate() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map(|e| e == "wgsl").unwrap_or(false) {
            let src = fs::read_to_string(&path).unwrap();
            let module = naga::front::wgsl::parse_str(&src)
                .unwrap_or_else(|e| panic!("{}: parse error: {e}", path.display()));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}: validation error: {e:?}", path.display()));
            checked += 1;
        }
    }
    assert!(checked > 0, "no shaders found in {}", dir.display());
}
