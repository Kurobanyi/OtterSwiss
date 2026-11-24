---
name: nueva-pantalla
description: Procedimiento paso a paso para diseñar, estructurar y programar una nueva pantalla, vista o componente reutilizable en Dioxus. Usar cuando el usuario pida crear, añadir o implementar una pantalla, vista, página, panel o componente de interfaz.
---

# Flujo de Creación de Nueva Pantalla o Componente

Esta guía describe el procedimiento obligatorio para incorporar una nueva pantalla o componente visual en la aplicación Dioxus.

## Procedimiento

### 1. Planificación y Diseño Previo
Antes de escribir cualquier línea de código en Rust:
- **Objetivo**: Define claramente el propósito de la pantalla o componente.
- **Jerarquía Visual**: Esboza la estructura del árbol de componentes (layout, paneles, contenedores).
- **Matriz de Estados**: Documenta cómo se representarán los estados de contenido normal, vacío (*empty*), en carga (*loading*) y de error (*error*).

### 2. Inspección y Reutilización
- Examina `src/ui/components/` para identificar botones, tarjetas, inputs o contenedores ya existentes.
- Reutiliza componentes antes de crear elementos visuales duplicados. Si un componente necesita una variante, extiende sus propiedades (`Props`) de forma retrocompatible.

### 3. Maquetación y Estilos con Tokens
- Prohibido incrustar estilos en línea con colores hexadecimales o píxeles directos en el macro `rsx!`.
- Añade las clases necesarias en `assets/main.css`.
- Utiliza exclusivamente las variables CSS existentes (`var(--bg)`, `var(--surface)`, `var(--accent)`, `var(--space-*)`, `var(--radius)`).

### 4. Cobertura Multi-Tema
- Comprueba que los contrastes y tonalidades funcionen correctamente en los tres temas definidos:
  - Claro (`[data-theme="light"]`)
  - Oscuro (`[data-theme="dark"]`)
  - Acento (`[data-theme="accent"]`)

### 5. Compilación y Validación Visual
- Compila el proyecto y ponlo en marcha con recarga rápida:
  ```bash
  dx serve
  ```
- Alterna entre al menos dos temas visuales distintos para verificar la consistencia del layout, la tipografía y los estados interactivos (`:hover`, `:focus-visible`).

### 6. Cierre y Auditoría
- Finaliza el desarrollo ejecutando la skill de revisión:
  ```text
  /revisar-ui
  ```
