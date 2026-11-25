//! Cuentas de progreso por pasos, iguales para el motor, el Debloater y el limpiador.

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
