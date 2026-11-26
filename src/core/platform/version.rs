//! Versiones numéricas con puntos («566.36», «31.0.101.5590») y su comparación (AR-25).

use std::cmp::Ordering;

/// Grupos de dígitos de una versión. Los ceros finales no cuentan: `1.2.0` y `1.2` son la misma versión.
#[derive(Debug, Clone)]
pub struct NumericVersion(Vec<u32>);

impl NumericVersion {
    /// Los grupos de dígitos de `text`, sin importar el separador. `None` si no hay ninguno («Desconocida», «Latest»),
    /// porque entonces no se sabe cuál es más nueva.
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Vec<u32> = text
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        (!parts.is_empty()).then_some(Self(parts))
    }

    /// `Some(true)` si `candidate` es más nueva que `current`; `None` si alguna de las dos no es numérica.
    pub fn is_newer(candidate: &str, current: &str) -> Option<bool> {
        Some(Self::parse(candidate)? > Self::parse(current)?)
    }
}

impl Ord for NumericVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.0.len().max(other.0.len());
        (0..len)
            .map(|i| {
                let a = self.0.get(i).copied().unwrap_or(0);
                let b = other.0.get(i).copied().unwrap_or(0);
                a.cmp(&b)
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for NumericVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NumericVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for NumericVersion {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_ceros_finales_no_cuentan() {
        assert_eq!(NumericVersion::parse("1.2.0"), NumericVersion::parse("1.2"));
        assert_ne!(NumericVersion::parse("1.2.1"), NumericVersion::parse("1.2"));
    }

    #[test]
    fn compara_por_numero_y_no_por_texto() {
        assert_eq!(NumericVersion::is_newer("566.36", "565.90"), Some(true));
        assert_eq!(NumericVersion::is_newer("9.5", "10.1"), Some(false));
        assert_eq!(NumericVersion::is_newer("566.36", "566.36"), Some(false));
    }

    #[test]
    fn sin_numeros_no_se_sabe() {
        assert_eq!(NumericVersion::is_newer("566.36", "Desconocida"), None);
        assert_eq!(NumericVersion::is_newer("Latest", "31.0.101.5590"), None);
    }
}
