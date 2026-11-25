//! TE-29: cada tema de `assets/main.css` cumple el contraste AA y las reglas de jerarquía. Una paleta nueva (como las
//! cuatro de v4.2.0) no entra si el texto no se lee, si peligro se confunde con el acento o si la superficie queda por
//! debajo del fondo.

use std::collections::HashMap;

const CSS: &str = include_str!("../assets/main.css");
/// WCAG 2.x AA para texto normal.
const AA: f64 = 4.5;

fn hex_luminance(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    let channel = |i: usize| {
        let c = f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap()) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (hex_luminance(a), hex_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Tokens con color hexadecimal de cada bloque `[data-theme="…"] { … }`.
fn themes() -> Vec<(String, HashMap<String, String>)> {
    let mut out = Vec::new();
    let mut rest = CSS;
    while let Some(start) = rest.find("[data-theme=\"") {
        let after = &rest[start + 13..];
        let name_end = after.find('"').unwrap();
        let name = after[..name_end].to_string();
        let body_start = after.find('{').unwrap() + 1;
        let body_end = after[body_start..].find('}').unwrap() + body_start;
        let tokens = after[body_start..body_end]
            .lines()
            .filter_map(|line| {
                let (key, value) = line.trim().split_once(':')?;
                let value = value.trim().trim_end_matches(';').trim();
                (key.starts_with("--") && value.starts_with('#') && value.len() == 7)
                    .then(|| (key.to_string(), value.to_string()))
            })
            .collect::<HashMap<_, _>>();
        // Solo los bloques que definen una paleta (hay reglas `[data-theme=…] .clase` con otros fines).
        if tokens.contains_key("--bg") {
            out.push((name, tokens));
        }
        rest = &after[body_end..];
    }
    out
}

#[test]
fn hay_temas_que_revisar() {
    assert!(themes().len() >= 10, "no se leyeron los temas de main.css");
}

#[test]
fn todos_los_temas_cumplen_aa_y_jerarquia() {
    let mut fallos = Vec::new();
    for (name, t) in themes() {
        let get = |k: &str| t.get(k).map(String::as_str);
        let mut check = |fg: &str, bg: &str| {
            if let (Some(a), Some(b)) = (get(fg), get(bg)) {
                let r = contrast(a, b);
                if r < AA {
                    fallos.push(format!("{name}: {fg} sobre {bg} = {r:.2}"));
                }
            }
        };
        for fg in [
            "--text",
            "--text-muted",
            "--accent-text",
            "--success-text",
            "--warning-text",
            "--danger-text",
        ] {
            check(fg, "--bg");
            check(fg, "--surface");
        }
        check("--on-accent", "--accent");
        check("--on-danger", "--danger");

        if get("--danger").is_some() && get("--danger") == get("--accent") {
            fallos.push(format!("{name}: --danger igual a --accent"));
        }
        if get("--warning").is_some() && get("--warning") == get("--accent-hover") {
            fallos.push(format!("{name}: --warning igual a --accent-hover"));
        }
        if let (Some(text), Some(muted)) = (get("--text"), get("--text-muted"))
            && contrast(text, muted) < 1.25
        {
            fallos.push(format!("{name}: --text y --text-muted casi iguales"));
        }
        if let (Some(bg), Some(surface)) = (get("--bg"), get("--surface"))
            && hex_luminance(surface) <= hex_luminance(bg)
        {
            fallos.push(format!("{name}: --surface no queda por encima de --bg"));
        }
    }
    assert!(
        fallos.is_empty(),
        "Temas fuera de regla:\n{}",
        fallos.join("\n")
    );
}
