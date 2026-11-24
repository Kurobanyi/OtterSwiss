---
name: revisar-ui
description: Auditoría visual y de código de interfaces Dioxus. Verifica contraste de color, escala de espaciado, soporte multi-tema y ausencia de estilos sueltos. Usar cuando se finalice una pantalla, tras modificar estilos CSS o cuando el usuario pida auditar, verificar o revisar la UI.
---

# Auditoría y Revisión de UI

Esta guía establece el protocolo para verificar la calidad visual, la coherencia de diseño y el cumplimiento de las reglas CSS en la interfaz de Dioxus.

## Pasos de Ejecución

### 1. Despliegue y Captura Multi-Tema
- Ejecuta la aplicación para inspeccionar la interfaz visual:
  - Si la vista contiene llamadas exclusivas de escritorio Win32:
    ```bash
    dx serve
    ```
  - Si los componentes no dependen de APIs nativas de escritorio, ejecútalo en navegador web para una inspección ágil:
    ```bash
    dx serve --platform web
    ```
- Revisa e inspecciona cada pantalla activa en los tres temas disponibles (Claro, Oscuro y Acento).

### 2. Inspección Visual y Responsive
Verifica minuciosamente los siguientes puntos:
- **Contraste de Texto**: Cumplimiento de la relación 4.5:1 (WCAG AA) sobre los fondos de cada tema.
- **Escala de Espaciado**: Márgenes y paddings ajustados a los tokens `--space-1` a `--space-5` (4, 8, 16, 24, 32px).
- **Alineación y Jerarquía**: Tamaños tipográficos y pesos visuales consistentes.
- **Estados de Interacción**: Foco accesible (`:focus-visible`), efectos hover y estado deshabilitado.
- **Redimensionamiento**: Comportamiento adaptativo de la ventana al cambiar su tamaño.

### 3. Detección Estática de Estilos Sueltos (Grep)
Ejecuta búsquedas para comprobar que no existan colores hexadecimales ni medidas arbitrarias en el código de componentes Rust:
- Buscar colores hexadecimales hardcodeados en Rust:
  ```powershell
  Get-ChildItem -Path src/ui -Recurse -Filter "*.rs" | Select-String -Pattern '#[0-9a-fA-F]{3,8}'
  ```
- Buscar dimensiones fijas en píxeles no tokenizadas:
  ```powershell
  Get-ChildItem -Path src/ui -Recurse -Filter "*.rs" | Select-String -Pattern '\b[0-9]+px\b'
  ```
Todo valor detectado debe reemplazarse por su token correspondiente en `assets/main.css`.

### 4. Corrección y Reporte
- Aplica los ajustes necesarios en `assets/main.css` o en los componentes de `src/ui/`.
- Proporciona un resumen conciso con la lista de mejoras o correcciones aplicadas.
