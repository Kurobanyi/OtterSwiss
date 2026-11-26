fn main() {
    println!("cargo:rerun-if-changed=assets/brand/icon.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/brand/icon.ico");
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
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
        // SEC-17: `asInvoker`; la app pide elevación solo para lo que la necesita (`relaunch_as_admin`). Solo el ejecutable
        // lleva el manifiesto: con `compile()` también lo heredaban los binarios de `cargo test`.
        // ERR-14: si esto falla, el .exe saldría sin manifiesto ni icono; mejor que el build falle.
        if let Err(e) = res.compile_for("otter_swiss") {
            panic!(
                "no se pudo incrustar el manifiesto y el icono (¿falta el SDK de Windows?): {e}"
            );
        }
    }
}
