fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}" />
    </application>
  </compatibility>
</assembly>"#,
        );
        // Solo el ejecutable lleva el manifiesto `requireAdministrator`: con `compile()` también lo heredaban
        // los binarios de `cargo test`, que entonces no arrancaban sin elevación (os error 740).
        // ERR-14: si esto falla, el .exe saldría sin manifiesto ni icono; mejor que el build falle.
        res.compile_for("otter_swiss")
            .expect("no se pudo incrustar el manifiesto y el icono (¿falta el SDK de Windows?)");
    }
}
