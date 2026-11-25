# Relevo compartido entre asistentes

Leer `AGENTS.md`, `docs/BUGS.md` y `git status`. Autoridades: `docs/BUGS.md` (índice, siguiente y patrones) y
`docs/BUGS-detalle.md` (expediente por ID, con línea **Cierre** en cada uno). Agentes en `.claude/agents/`
(`security-auditor`, `code-auditor`, `design-challenger`, `commit-writer`).

## ★ Estado actual — 2026-11-24

**Estado:** arreglos de la auditoría del 24/11 commiteados en **`fix/auditoria`** (15 commits sobre `11cb659`, cada árbol
compilado por separado) y en revisión en el PR https://github.com/Kurobanyi/OtterSwiss/pull/1. 83 de 90 hallazgos resueltos;
2 parciales (`TST-01`, `UI-11`), 4 diferidos (`SEC-17`, `SYS-24`, `ARQ-03`, `UX-02`) y 1 pendiente de decisión (`UX-03`).
El repo no tiene CI.

**Hecho el 24/11 (sin commitear, en `fix/auditoria`):**
- `ARQ-04` resuelto: CRT estático (decisión del usuario). `.cargo/config.toml` vuelve a versionarse solo con `+crt-static` para
  `x86_64-pc-windows-msvc`; las rutas propias de `--remap-path-prefix` van en el `~/.cargo/config.toml` de cada equipo (Cargo une los
  arrays). `dumpbin /dependents`: sin `VCRUNTIME140*`/`ucrtbase`. Probado en Sandbox sin DLL de VC.
- `ERR-13` resuelto: `main.rs` comprueba WebView2 antes de Dioxus y ofrece instalarlo con un `MessageBoxW` nativo; hook de `panic` con
  cuadro nativo. `is_webview2_installed` usa `GetAvailableCoreWebView2BrowserVersionString` (dependencia directa `webview2-com` 0.38, ya
  estaba en el árbol): el registro del Sandbox daba un falso «instalado». `ProcessExit::code_display` muestra HRESULT en hex.
- `SYS-04` reabierto y cerrado: con la app registrada, el barrido de Program Files ya no ofrece su carpeta.
- `tests/vm_sistema.rs` (`#[ignore]`): recorrido del núcleo como admin. Corrido en Windows Sandbox: **0 fallos** (ver `docs/BUGS.md`,
  Resumen). Sin probar: quitar una app Appx y la ventana chica.

**Siguiente paso:** el usuario decide si commitear estos cambios en `fix/auditoria` (no commitear ni hacer push sin pedido). Luego, en un
Windows completo: quitar una app Appx con el Debloater y ver la ventana en tamaño chico.

**VM:** VirtualBox no sirve en esta PC (el hipervisor de Windows está activo: VirtualBox cae a NEM, el EFI se cuelga en `DXE_AP` con 2
núcleos y Windows 11 exige 2). Quedan `D:\VMs\OtterTest-Win11` (VM apagada) y la ISO verificada `D:\VMs\Win11_Enterprise_Eval_es-es.iso`
(SHA-256 oficial `8F344BEA…E4C584`); el usuario decide si borrarlas. Para un Windows completo: activar Hyper-V (pide reinicio).

**Windows Sandbox (si se reusa):** característica ya activada. Lanzar con `WindowsSandbox.exe <wsb>`. NUNCA matar sus procesos (deja VMs
huérfanas); se cierra con la X. La prueba eleva con `RunAs`: el usuario acepta el UAC dentro. El control de pantalla no ve esa ventana.

**Después de los bugs (pedido del usuario):** auditoría de buenas prácticas: arquitectura y orden del código, código limpio y
reutilizable, temas claro/oscuro y tokens bien hechos, i18n, linters y similares. Va en una tarea propia, no en este PR.

## Cómo verificar

El usuario **apagó Smart App Control** el 24/11: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build` y
`cargo fmt` corren en Windows (PowerShell; nunca desde Git Bash, donde `link` es el de coreutils). **Último resultado: 37
tests y clippy en verde.** `.agents/scripts/check-wsl.sh` queda como alternativa desde WSL (solo compila).

Para recorrer la interfaz: la app pide administrador y Windows (UIPI) no deja controlar ventanas elevadas, así que se compila
una copia con `asInvoker` en `$env:TEMP\otter-ui-target` (cambiando `build.rs` solo para esa compilación y restaurándolo) y se
recorre sin elevación. Recorrido hecho: todas las pestañas en oscuro, Dashboard y Limpiador en claro e inglés, diálogos con
foco atrapado y Escape. Falta: acciones de sistema como admin (mejor en VM o Windows Sandbox) y ventana en tamaño chico.

## Decisiones del usuario (2026-11-24)

- `SEC-17` (asInvoker + auxiliar elevado) y con él `SYS-24`: diferidos.
- `UX-02` (qué ajustes salen del perfil automático): diferido. Se conserva la confirmación con el plan antes de optimizar.
- `UX-03` (Optimizar RAM): pendiente, el usuario avisará.
- `ARQ-01`: `.cargo/config.toml` fuera del índice y en `.gitignore`; no se reescribe el historial.
- `ARQ-02`: changelog renombrado a `CHANGELOG.md` y completado.
- `ARQ-03`: `drivers.rs` se conserva; se usará para actualizar drivers desde la app.
- `UI-11`: los textos del núcleo en inglés quedan para otro PR.
- qBittorrent: se descargó el instalador 5.1.4 y se fijó su SHA-256 (no trae Authenticode; firma PGP).

## Ojo

- Las acciones que cambian el sistema no se ejecutaron: probarlas como administrador antes de publicar una versión.
- `rust-version` subió a 1.93 por `sevenz-rust2` 0.25.
- `Cargo.lock` cambia por la versión del paquete (3.5.1) y la migración de 7z.
