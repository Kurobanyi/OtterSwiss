---
trigger: glob
globs: "*.css, src/ui/**/*.rs"
description: "Reglas de arquitectura de UI, tokens CSS, componentes y gestión de temas dinámicos en Dioxus."
---

# Reglas de UI y Sistema de Temas

## 1. Arquitectura de Temas Dinámicos
- **Estado del Tema**: El tema activo debe gestionarse mediante `Signal<Theme>` registrado en el contexto global de Dioxus (`use_context` / `provide_context`).
- **Aplicación en DOM**: `data-theme` con el tema concreto (`Theme::css_name`) en la raíz de la app y en `<html>` (`ui::document::sync_document_attrs`), para que `color-scheme` llegue a barras y controles nativos. «Según Windows» (`Theme::System`) se resuelve a Claro u Oscuro al cargar.
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
  - Espaciado: `--space-1` a `--space-5`
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

## 6. Clases antes que estilos inline
- Layout y texto con las utilidades de `main.css` (`stack-*`, `row`, `row-between`, `list-row`, `text-*`, `fw-*`, `mb-*`, `input`,
  `btn-sm`); `style:` solo para valores calculados (ancho de una barra, por ejemplo).
- Texto de estado o acento siempre con `--*-text` (o `.text-success|warning|danger|accent`); `--success`, `--warning`, `--danger`
  y `--accent` son para fondos y bordes (UI-18).
- Una opción elegida dentro de un grupo se marca con `aria-pressed`, no alternando `btn-primary` (TE-13).
- `--bg` de cada tema tiene que coincidir con `Theme::background_rgba` (lo comprueba un test).

## 7. Textos
- Los textos de la interfaz van en `ui/i18n/strings.rs` (`Strings`, un `static` por idioma); si falta uno, no compila.
- Español con mayúscula solo inicial, «…» en vez de «...», plurales con `plural()`.

## 8. Lenguaje visual (Windows 11 / Fluent, decidido el 26/11/2026)
- **Estructura de la ventana** (`TE-31`): barra de título con la nutria (arrastra la ventana), las secciones, idioma, tema,
  engranaje de Configuración y botones de ventana. Por defecto las secciones van **arriba** (pedido del dueño); en
  Configuración se pasan a una barra lateral contraíble (`LayoutPrefs`, `data-nav` y `data-collapsed` en la raíz). El shell pinta
  el encabezado de cada página (icono, título y descripción) y el aviso de modo estándar: una vista no repite su nombre.
- **Estilo abierto** (`TE-31`): nada de una caja por elemento. Estructura con espacio, tipografía y líneas finas; la primera
  tarjeta de la página (la acción principal) es el único punto de color. Las reglas globales están en «9 · Estilo abierto» de
  `main.css`; una vista nueva no vuelve a poner recuadros a sus filas.
- **Jerarquía de una página**: encabezado del shell → acciones principales de la página → grupos con `.section-title`
  (texto 14 seminegrita) y debajo tarjetas o filas. Como Configuración de Windows: filas `.item-row` (icono, título, descripción
  y control a la derecha) en lugar de tarjetas para todo.
- **Tamaños**: 12 (pie, metadatos), 14 (cuerpo y títulos de tarjeta), 20 (título de diálogo, cifras destacadas), 28 (título de
  página). Pesos 400 y 600. Nada de MAYÚSCULAS en etiquetas.
- **Controles**: altura 32 (`.btn`, `.input`), 28 en `.btn-sm`; radio 4 en controles y 8 en tarjetas, filas y diálogos.
  Un solo botón de acento (`.btn-primary`) por grupo: la acción principal. El resto, `.btn` estándar. Lo destructivo, `.btn-danger`
  y siempre con confirmación.
- **Superficies**: `--layer` (fondo de página), `--surface` (tarjeta, fila), `--stroke` (trazo y separadores), `--control-fill*`
  (relleno de controles), `--subtle-fill-*` (hover de elementos sin fondo). Se derivan del tema: no hace falta definirlas en
  cada tema.
- **Avisos**: `InfoBar` (componente) para lo persistente de una página (permisos, estado); `StatusBanner` para el resultado de
  una acción; `badge-*` para estados cortos de una fila.
- **Iconos**: glifos de Segoe Fluent Icons con `.fluent-icon` y `aria-hidden`; cada botón solo-icono lleva `aria-label`.
- **CSS de una vista**: lo que no sea un componente común va en la sección «8 · Vistas» de `assets/main.css`, con un
  comentario por vista; antes de crear una clase, reutiliza las comunes.
