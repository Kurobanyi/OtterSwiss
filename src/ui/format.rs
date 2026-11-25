//! CL-12: formato de tamaños y duraciones para mostrar. Un solo criterio en toda la app.

const MIB: f64 = 1024.0 * 1024.0;

/// Bytes en MB: «0 MB», «< 0.01 MB», «12.34 MB» o «123 MB» (sin decimales desde 100 MB).
pub fn bytes_mb(bytes: u64) -> String {
    let mb = bytes as f64 / MIB;
    if bytes == 0 {
        "0 MB".to_string()
    } else if mb < 0.01 {
        "< 0.01 MB".to_string()
    } else if mb >= 100.0 {
        format!("{:.0} MB", mb)
    } else {
        format!("{:.2} MB", mb)
    }
}

/// Bytes en MB con un decimal, para contadores de progreso («12.3»).
pub fn mb_1(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / MIB)
}

/// Capacidad en MB a texto: «512 MB», «8 GB», «1.5 GB».
pub fn capacity_mb(mb: u32) -> String {
    if mb >= 1024 && mb.is_multiple_of(1024) {
        format!("{} GB", mb / 1024)
    } else if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{} MB", mb)
    }
}

/// Tiempo restante aproximado: «~45 s», «~3 min 20 s», «—» si no se sabe.
pub fn eta(seconds: Option<u64>) -> String {
    match seconds {
        None => "—".to_string(),
        Some(s) if s < 60 => format!("~{s} s"),
        Some(s) => format!("~{} min {} s", s / 60, s % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tamanos() {
        assert_eq!(bytes_mb(0), "0 MB");
        assert_eq!(bytes_mb(1), "< 0.01 MB");
        assert_eq!(bytes_mb(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(bytes_mb(150 * 1024 * 1024), "150 MB");
        assert_eq!(capacity_mb(512), "512 MB");
        assert_eq!(capacity_mb(8192), "8 GB");
        assert_eq!(capacity_mb(1536), "1.5 GB");
    }

    #[test]
    fn tiempo_restante() {
        assert_eq!(eta(None), "—");
        assert_eq!(eta(Some(45)), "~45 s");
        assert_eq!(eta(Some(3600)), "~60 min 0 s");
    }
}
