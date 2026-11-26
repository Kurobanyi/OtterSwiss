//! IN-02: textos de la vista dns. Un campo por texto; el compilador exige los dos idiomas.

pub struct DnsStrings {
    pub apply_dns: &'static str,
    pub automatic_dhcp: &'static str,
    pub connect_to_a_network_and: &'static str,
    pub could_not_read_the_adapters: &'static str,
    pub dns_optimizer: &'static str,
    pub it_replaces_the_dns_of: &'static str,
    pub measuring_ping: &'static str,
    pub network_adapters: &'static str,
    pub no_active_adapters: &'static str,
    pub no_server_answered_the_ping: &'static str,
    pub physical: &'static str,
    pub pinging: &'static str,
    pub public_dns_servers_and_ping: &'static str,
    pub reading_network_adapters: &'static str,
    pub restore_previous: &'static str,
    pub test_ping: &'static str,
    pub there_are_no_active_physical: &'static str,
    pub lowest_ping_ms: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub apply: fn(&dyn std::fmt::Display) -> String,
    pub preset_applied: fn(&dyn std::fmt::Display, &dyn std::fmt::Display) -> String,
    pub previous_dns_restored: fn(&dyn std::fmt::Display) -> String,
}

pub static ES: DnsStrings = DnsStrings {
    apply_dns: "Aplicar DNS",
    automatic_dhcp: "automático (DHCP)",
    connect_to_a_network_and: "Conéctate a una red y vuelve a abrir esta pestaña.",
    could_not_read_the_adapters: "No se pudieron leer los adaptadores",
    dns_optimizer: "Optimizador DNS",
    it_replaces_the_dns_of: "Reemplaza el DNS de estos adaptadores. En una red de empresa o con un DNS propio (Pi-hole, router) algunos nombres pueden dejar de resolverse; «Restaurar DNS» vuelve al anterior.",
    measuring_ping: "Midiendo el ping…",
    network_adapters: "Adaptadores de Red",
    no_active_adapters: "Sin adaptadores activos",
    no_server_answered_the_ping: "Ningún servidor respondió al ping.",
    physical: "Físico",
    pinging: "Midiendo…",
    public_dns_servers_and_ping: "Servidores DNS públicos y prueba de ping",
    reading_network_adapters: "Leyendo los adaptadores de red…",
    restore_previous: "Restaurar DNS",
    test_ping: "Probar Ping",
    there_are_no_active_physical: "No hay adaptadores físicos activos.",
    lowest_ping_ms: |a0, best_ms| format!("Menor ping: {a0} ({best_ms} ms)"),
    apply: |a0| format!("¿Aplicar {a0}?"),
    preset_applied: |name, adapters| format!("{name} aplicado a {adapters}."),
    previous_dns_restored: |adapters| format!("DNS anterior restaurado en {adapters}."),
};

pub static EN: DnsStrings = DnsStrings {
    apply_dns: "Apply DNS",
    automatic_dhcp: "automatic (DHCP)",
    connect_to_a_network_and: "Connect to a network and refresh this tab.",
    could_not_read_the_adapters: "Could not read the adapters",
    dns_optimizer: "DNS Optimizer",
    it_replaces_the_dns_of: "It replaces the DNS of these adapters. On a company network or with your own DNS (Pi-hole, router) some names may stop resolving; «Restore DNS» brings back the previous one.",
    measuring_ping: "Measuring ping…",
    network_adapters: "Network Adapters",
    no_active_adapters: "No active adapters",
    no_server_answered_the_ping: "No server answered the ping.",
    physical: "Physical",
    pinging: "Pinging…",
    public_dns_servers_and_ping: "Public DNS servers and ping test",
    reading_network_adapters: "Reading network adapters…",
    restore_previous: "Restore Previous",
    test_ping: "Test Ping",
    there_are_no_active_physical: "There are no active physical adapters.",
    lowest_ping_ms: |a0, best_ms| format!("Lowest ping: {a0} ({best_ms} ms)"),
    apply: |a0| format!("Apply {a0}?"),
    preset_applied: |name, adapters| format!("{name} applied to {adapters}."),
    previous_dns_restored: |adapters| format!("Previous DNS restored on {adapters}."),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Ningún texto vacío en ningún idioma.
    #[test]
    fn textos_completos() {
        for s in [&ES, &EN] {
            for f in [
                s.apply_dns,
                s.automatic_dhcp,
                s.connect_to_a_network_and,
                s.could_not_read_the_adapters,
                s.dns_optimizer,
                s.it_replaces_the_dns_of,
                s.measuring_ping,
                s.network_adapters,
                s.no_active_adapters,
                s.no_server_answered_the_ping,
                s.physical,
                s.pinging,
                s.public_dns_servers_and_ping,
                s.reading_network_adapters,
                s.restore_previous,
                s.test_ping,
                s.there_are_no_active_physical,
            ] {
                assert!(!f.trim().is_empty());
            }
        }
    }
}
