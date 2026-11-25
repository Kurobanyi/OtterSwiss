//! Cuentas de progreso por pasos, iguales para el motor, el Debloater y el limpiador.

use std::sync::{Arc, Mutex};

/// CL-07: progreso por pasos que comparten el motor, el Debloater y el limpiador.
pub trait StepProgress {
    /// Fija el paso actual. `percent` ya viene calculado; cada tipo puede acotarlo a su manera.
    fn set_step(&mut self, step: usize, total: usize, label: &str, percent: u8);
}

/// Publica el paso `step` de `total` con su etiqueta, si hay a quién informar.
pub fn report_step<P: StepProgress>(
    progress: Option<&Arc<Mutex<P>>>,
    step: usize,
    total: usize,
    label: &str,
) {
    if let Some(shared) = progress
        && let Ok(mut p) = shared.lock()
    {
        p.set_step(step, total.max(1), label, step_percent(step, total));
    }
}

/// Porcentaje de `done` pasos sobre `total` (0–100). Con `total` 0 se toma como 1 para no dividir por cero.
pub fn step_percent(done: usize, total: usize) -> u8 {
    ((done * 100) / total.max(1)).min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcentaje_acotado_y_sin_division_por_cero() {
        assert_eq!(step_percent(0, 0), 0);
        assert_eq!(step_percent(1, 4), 25);
        assert_eq!(step_percent(9, 4), 100);
    }
}
