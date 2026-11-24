# Contexto y Directrices del Proyecto

Este proyecto es una aplicación de escritorio desarrollada en Rust utilizando **Dioxus 0.7** (renderer desktop) en entorno Windows.

## Arquitectura del Proyecto

- `src/core/`: Lógica de negocio y servicios del sistema. Debe ser completamente pura e independiente de Dioxus y de la interfaz gráfica.
- `src/ui/`: Vistas, pantallas y módulos de interfaz en Dioxus.
- `src/ui/components/`: Componentes reutilizables de UI.
- `assets/main.css`: Estilos globales y tokens CSS organizados por temas.
- El usuario final puede seleccionar y alternar libremente su tema visual preferido.

## Comandos Principales de Desarrollo

- `dx serve`: Compilar y ejecutar la aplicación en desarrollo con soporte de hot-reload.
- `cargo fmt`: Formatear automáticamente todo el código fuente.
- `cargo clippy -- -D warnings`: Ejecutar linter con detección estricta de advertencias como errores.
- `cargo test`: Ejecutar pruebas unitarias y de integración.

## Consulta Obligatoria de Documentación de Versión

Dioxus ha experimentado cambios drásticos en su API entre versiones mayores (especialmente en signals, hooks, gestión de estado y backend desktop). Es mandatorio consultar la documentación oficial correspondiente a la versión exacta declarada en `Cargo.toml` en lugar de programar llamadas de memoria a versiones anteriores de la API.
