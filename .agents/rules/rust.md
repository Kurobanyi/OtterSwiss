---
trigger: glob
globs: "*.rs, Cargo.toml"
description: "Reglas de desarrollo en Rust: edición 2021+, gestión de errores, estado con enums y core desacoplado de la UI."
---

# Reglas de Desarrollo en Rust

1. **Edición de Rust**:
   - Todo el código debe adherirse a la edición 2021 o superior de Rust.

2. **Gestión Robusta de Errores**:
   - Prohibido el uso de `.unwrap()` y `.expect()` fuera de los bloques de pruebas (`#[cfg(test)]`).
   - Usar `thiserror` para definir errores de dominio tipados y estructurados en `src/core/`.
   - Usar `anyhow::Result` y `anyhow::Context` para el manejo y propagación de errores en capas de aplicación y UI.

3. **Modelado de Estado**:
   - Modelar estados de componentes, tareas y procesos mediante `enum` para hacer que los estados inválidos sean irrepresentables.

4. **Optimización de Recursos**:
   - Evitar clonaciones (`.clone()`) innecesarias. Priorizar préstamos (`&T`, `&str`) y paso por referencia.

5. **Control de Dependencias**:
   - No añadir dependencias a `Cargo.toml` sin justificación explícita y verificación previa de compatibilidad de versión.

6. **Desacoplamiento de Arquitectura**:
   - El módulo `src/core/` no debe importar ni depender de Dioxus ni de ningún elemento de presentación. Debe ser consumible de forma agnóstica.
