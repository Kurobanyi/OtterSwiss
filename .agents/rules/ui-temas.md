---
trigger: glob
globs: "*.css, src/ui/**/*.rs"
description: "Reglas de arquitectura de UI, tokens CSS, componentes y gestión de temas dinámicos en Dioxus."
---

# Reglas de UI y Sistema de Temas

## 1. Arquitectura de Temas Dinámicos
- **Estado del Tema**: El tema activo debe gestionarse mediante `Signal<Theme>` registrado en el contexto global de Dioxus (`use_context` / `provide_context`).
- **Aplicación en DOM**: Se aplica a través del atributo `data-theme="nombre"` en el contenedor raíz de la aplicación.
- **Definición en CSS**: Cada tema se define en `assets/main.css` mediante un bloque selector `[data-theme="nombre"]`.
- **Temas Obligatorios**: La aplicación debe incluir como mínimo 3 temas completos:
  - Claro (Light)
  - Oscuro (Dark)
  - Acento / Alto Contraste (ej. Cyberpunk, Neón o Esmeralda)
- **Persistencia**: La selección del tema se persiste en disco local en formato JSON/TOML mediante `serde` y la biblioteca `dirs`, cargándose inmediatamente al iniciar la app.

## 2. Sistema de Tokens y Variables CSS
- **Prohibición de Valores Sueltos**: Está estrictamente prohibido utilizar colores hexadecimales (`#...`), tamaños en píxeles fijos o sombras sueltas en el código de componentes Rust (`src/ui/`).
- **Tokens Obligatorios**: Todo estilo debe derivar de variables CSS estándar:
  - Paleta y Superficies: `--bg`, `--surface`, `--text`, `--accent`, `--border`
  - Espaciado: `--space-1` a `--space-6`
  - Bordes: `--radius`
- **Consistencia Total**: Cualquier nuevo token agregado a `assets/main.css` debe definirse obligatoriamente en **TODOS** los temas disponibles.

## 3. Escala Visual y Tipografía
- **Escala de Espaciado**: Utilizar estrictamente la escala de 4px, 8px, 16px, 24px y 32px (mapeada a `--space-1` hasta `--space-5`).
- **Tipografía**: Una única familia tipográfica para toda la aplicación y entre 4 y 5 tamaños normalizados.

## 4. Estados Obligatorios
- **Componentes Interactivos**: Botones, enlaces e inputs deben implementar obligatoriamente estilos para:
  - Estado normal
  - `:hover`
  - Foco visible (`:focus-visible`)
  - Deshabilitado (`:disabled`)
- **Pantallas y Vistas**: Toda pantalla o módulo principal debe contemplar explícitamente:
  - Estado vacío (*Empty state*)
  - Estado cargando (*Loading state*)
  - Estado de error (*Error state*)

## 5. Accesibilidad y Reutilización
- **Contraste Mínimo**: Garantizar una relación de contraste de texto mínima de 4.5:1 (estándar WCAG AA) en todos y cada uno de los temas.
- **Reutilización**: Antes de crear un componente nuevo, se deben revisar y reutilizar los existentes en `src/ui/components/`.
