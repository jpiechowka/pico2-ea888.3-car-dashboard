//! Compile-time identity policy selects the distinct explicit utility.
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/tool.rs"]
/// Actual utility policy declarations; no hardware or executor is instantiated.
mod tool;

/// Same fixed policy supplied by the identity binary, never selected by a peer.
const TOOL_KIND: tool::ToolKind = tool::ToolKind::NgmIdentity;
/// Product selection must be evaluable before any runtime initialization.
const PRODUCT: &str = tool::tool_kind().usb_product();

/// Identity inspection and USB identity cannot fall back to standard before boot.
#[test]
fn identity_policy_is_available_at_compile_time() {
    assert_eq!(PRODUCT, "Pico NGM Identity Captures");
    assert_eq!(tool::tool_kind().inspect_label(), "Inspect NGM INFO");
    assert_eq!(tool::tool_kind() as u8, 1);
}
