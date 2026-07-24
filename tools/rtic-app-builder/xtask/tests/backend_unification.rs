use std::path::Path;

#[test]
fn osd_adapter_uses_the_canonical_msp_crate_without_a_local_copy() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has a repository parent");
    let adapter_manifest =
        std::fs::read_to_string(repository_root.join("compat/ferrowasp-serial-osd/Cargo.toml"))
            .expect("read serial/OSD adapter manifest");

    assert!(
        adapter_manifest
            .contains("ferrowasp-mspv1 = { path = \"../../../../crates/ferrowasp-mspv1\" }"),
        "the OSD adapter must use FerroWasp's canonical MSP crate"
    );
    assert!(
        !repository_root.join("compat/ferrowasp-mspv1").exists(),
        "a builder-local MSP copy must not be reintroduced"
    );
}
