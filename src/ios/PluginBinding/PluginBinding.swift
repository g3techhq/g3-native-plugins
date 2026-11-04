// Intentionally empty. Each plugin's `#[manganis::ffi]` block asks `dx` to build
// a product named after the plugin; those products are static libraries of this
// target, so they hold nothing and `dx` does not install them. The plugins' Swift
// is in the product named after the enabled features. See Package.swift.
