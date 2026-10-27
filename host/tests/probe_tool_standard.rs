//! Compile-time standard policy remains independent of identity code availability.
#[allow(dead_code)]
#[path = "../../firmware/ble-probe/src/tool.rs"]
/// Actual utility policy declarations; no hardware or executor is instantiated.
mod tool;

/// Same fixed policy supplied by the ordinary binary, never mutable state.
const TOOL_KIND: tool::ToolKind = tool::ToolKind::StandardProbe;
/// Product selection must be evaluable before any runtime initialization.
const PRODUCT: &str = tool::tool_kind().usb_product();

/// Standard inspection and USB identity cannot depend on an initialization call.
#[test]
fn standard_policy_is_available_at_compile_time() {
    assert_eq!(PRODUCT, "Pico BLE Probe Captures");
    assert_eq!(tool::tool_kind().inspect_label(), "Inspect GATT");
    assert_eq!(tool::tool_kind() as u8, 0);
}
