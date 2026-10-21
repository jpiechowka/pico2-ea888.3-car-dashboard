//! ## Generic Access Profile
//!
//! This profile defines the generic procedures related to discovery of
//! Bluetooth devices (idle mode procedures) and link management aspects
//! of connecting to Bluetooth devices (connecting mode procedures).
//! It also defines procedures related to use of different security levels.
//! In addition, this profile includes common format requirements for
//! parameters accessible on the user interface level.
//! This builder imposes a 22-byte name limit; [GAP section 12.1](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-62/out/en/host/generic-access-profile.html) permits up to 248 bytes.

use embassy_sync::blocking_mutex::raw::RawMutex;

use crate::prelude::*;

/// Local convenience limit in UTF-8 bytes, chosen to leave nine bytes in legacy advertising.
/// The GAP Device Name characteristic itself has a different specification limit.
const DEVICE_NAME_MAX_LENGTH: usize = 22;

/// Six attributes: GAP service, name and appearance declaration/value pairs, then empty GATT service.
#[cfg(not(feature = "security"))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 6;
/// Six attributes when security is enabled without the central-address-resolution characteristic.
#[cfg(all(feature = "security", not(feature = "central")))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 6;
/// Eight attributes when security and central support add address-resolution declaration/value.
#[cfg(all(feature = "security", feature = "central"))]
pub const GAP_SERVICE_ATTRIBUTE_COUNT: usize = 8;

/// Configuration for the GAP Service.
pub enum GapConfig<'a> {
    /// Peripheral device configuration.
    Peripheral(#[doc = "Borrowed name/appearance used to create the common GAP/GATT services."] PeripheralConfig<'a>),
    /// Central device configuration.
    Central(#[doc = "Borrowed name/appearance; attributes currently match the peripheral builder."] CentralConfig<'a>),
}

/// Configuration for a peripheral device GAP Service.
pub struct PeripheralConfig<'a> {
    /// Borrowed UTF-8 name, at most 22 bytes for build admission.
    pub name: &'a str,
    /// The representation of the external appearance of the device.
    ///
    /// Example: `&appearance::sensor::GENERIC_SENSOR.`
    pub appearance: &'a BluetoothUuid16,
    // TODO: Add more GAP parameters
    // pub preferred_connection_parameters: Option<ConnectionParameters>,
}

/// Configuration for a central device GAP Service.
pub struct CentralConfig<'a> {
    /// Borrowed UTF-8 name, at most 22 bytes for build admission.
    pub name: &'a str,
    /// The representation of the external appearance of the device.
    ///
    /// Example: `&appearance::sensor::GENERIC_SENSOR`
    pub appearance: &'a BluetoothUuid16,
    // TODO: Add more GAP parameters
}

impl<'a> GapConfig<'a> {
    /// Returns peripheral configuration borrowing `name` with UNKNOWN appearance; length is checked by build.
    pub fn default(name: &'a str) -> Self {
        GapConfig::Peripheral(PeripheralConfig {
            name,
            appearance: &appearance::UNKNOWN,
        })
    }

    /// Adds GAP/GATT services to `table`, returning Err before mutation if the name exceeds 22 bytes.
    /// Capacity exhaustion panics after possible partial insertion; name/appearance bytes remain borrowed.
    pub fn build<M: RawMutex, const MAX: usize>(
        self,
        table: &mut AttributeTable<'a, M, MAX>,
    ) -> Result<(), &'static str> {
        match self {
            GapConfig::Peripheral(config) => config.build(table),
            GapConfig::Central(config) => config.build(table),
        }
    }
}

impl<'a> PeripheralConfig<'a> {
    /// Appends the configured GAP and empty GATT services to `table`, returning Ok on completion.
    /// Rejects names over 22 bytes before writes; insufficient table capacity panics with partial entries retained.
    fn build<M: RawMutex, const MAX: usize>(self, table: &mut AttributeTable<'a, M, MAX>) -> Result<(), &'static str> {
        // Store the name by reference (lives for `'a`); a one-shot StaticCell
        // would panic if the server is ever built twice.
        if self.name.len() > DEVICE_NAME_MAX_LENGTH {
            return Err("Device name is too long. Max length is 22 bytes");
        }

        let mut gap_builder = table.add_service(Service::new(service::GAP));
        gap_builder.add_characteristic_ro(characteristic::DEVICE_NAME, self.name);
        gap_builder.add_characteristic_ro(characteristic::APPEARANCE, self.appearance);
        #[cfg(all(feature = "security", feature = "central"))]
        gap_builder.add_characteristic_ro(characteristic::CENTRAL_ADDRESS_RESOLUTION, &1u8);
        gap_builder.build();

        table.add_service(Service::new(service::GATT));

        Ok(())
    }
}

impl<'a> CentralConfig<'a> {
    /// Appends the configured GAP and empty GATT services to `table`, returning Ok on completion.
    /// Rejects names over 22 bytes before writes; insufficient table capacity panics with partial entries retained.
    fn build<M: RawMutex, const MAX: usize>(self, table: &mut AttributeTable<'a, M, MAX>) -> Result<(), &'static str> {
        // Store the name by reference (lives for `'a`); a one-shot StaticCell
        // would panic if the server is ever built twice.
        if self.name.len() > DEVICE_NAME_MAX_LENGTH {
            return Err("Device name is too long. Max length is 22 bytes");
        }

        let mut gap_builder = table.add_service(Service::new(service::GAP));
        gap_builder.add_characteristic_ro(characteristic::DEVICE_NAME, self.name);
        gap_builder.add_characteristic_ro(characteristic::APPEARANCE, self.appearance);
        #[cfg(all(feature = "security", feature = "central"))]
        gap_builder.add_characteristic_ro(characteristic::CENTRAL_ADDRESS_RESOLUTION, &1u8);
        gap_builder.build();

        table.add_service(Service::new(service::GATT));

        Ok(())
    }
}

#[cfg(test)]
/// Repeated construction and UTF-8 byte-length admission tests.
mod tests {
    extern crate std;

    use embassy_sync::blocking_mutex::raw::NoopRawMutex;

    use super::*;

    // Building the GAP config twice must not panic. With the one-shot StaticCell
    // the second build panicked ("StaticCell already full").
    #[test]
    /// Borrowed characteristic storage permits repeated service-table construction.
    fn gap_config_can_be_built_twice() {
        for _ in 0..2 {
            let mut table: AttributeTable<'_, NoopRawMutex, 16> = AttributeTable::new();
            GapConfig::Peripheral(PeripheralConfig {
                name: "trouble",
                appearance: &appearance::UNKNOWN,
            })
            .build(&mut table)
            .unwrap();
        }
    }
    /// Name admission counts UTF-8 bytes and rejects before mutating either role's table.
    #[test]
    fn name_byte_bounds_preserve_empty_table_on_rejection() {
        for name in [
            "",
            "abcdefghijklmnopqrstuv",
            "ééééééééééé",
            "abcdefghijklmnopqrstuvw",
            "éééééééééééé",
        ] {
            for peripheral in [false, true] {
                let mut table: AttributeTable<'_, NoopRawMutex, 16> = AttributeTable::new();
                let config = if peripheral {
                    GapConfig::Peripheral(PeripheralConfig {
                        name,
                        appearance: &appearance::UNKNOWN,
                    })
                } else {
                    GapConfig::Central(CentralConfig {
                        name,
                        appearance: &appearance::UNKNOWN,
                    })
                };
                let accepted = config.build(&mut table).is_ok();
                assert_eq!(accepted, name.len() <= DEVICE_NAME_MAX_LENGTH);
                assert_eq!(table.uuid(1).is_some(), accepted);
                if accepted {
                    assert!(table.uuid(GAP_SERVICE_ATTRIBUTE_COUNT as u16).is_some());
                }
            }
        }
    }
}
