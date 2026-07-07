fn main() {
    // Emits the platform link args an extension module needs (notably
    // `-undefined dynamic_lookup` on macOS) so plain `cargo build` links.
    pyo3_build_config::add_extension_module_link_args();
}
