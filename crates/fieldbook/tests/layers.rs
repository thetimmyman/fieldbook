//! The crate chain links in the order the architecture fixes.

#[test]
fn the_chain_is_calc_core_render() {
    assert_eq!(
        [
            fieldbook_calc::LAYER,
            fieldbook_core::LAYER,
            fieldbook_render::LAYER
        ],
        ["calc", "core", "render"]
    );
}
