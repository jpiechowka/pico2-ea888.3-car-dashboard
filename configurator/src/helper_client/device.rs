//! Device confirmation is a user choice bound to a single inspection snapshot.
use dashboard_helper_protocol::*;

use super::Connection;

#[derive(Clone, Default)]
/// Selected BOOTSEL descriptor, matching inspection and explicit board confirmation.
/// Replacing selection or inspection clears any earlier confirmation.
pub struct DeviceSelection {
    /// Descriptor-only device chosen by the user, absent before selection/reset.
    selected: Option<DeviceSummary>,
    /// Validated inspection matching the selected descriptor and current helper session.
    inspection: Option<DeviceInspection>,
    /// User-confirmed genuine Pico 2 W contract bound to the inspection information hash.
    acknowledgement: Option<BoardAcknowledgement>,
}
impl DeviceSelection {
    /// Returns the selected descriptor, or `None` after reset/failed selection.
    pub fn selected(&self) -> Option<&DeviceSummary> { self.selected.as_ref() }

    /// Returns the matching accepted inspection, including incomplete reports if present.
    pub fn inspection(&self) -> Option<&DeviceInspection> { self.inspection.as_ref() }

    /// Returns the explicit board acknowledgement, or `None` until confirmed anew.
    pub fn acknowledgement(&self) -> Option<&BoardAcknowledgement> { self.acknowledgement.as_ref() }

    /// Discard selection, inspection and board confirmation together.
    pub fn clear(&mut self) { *self = Self::default(); }

    /// Clear prior state and select `device` only when descriptor-only with RP2350
    /// BOOTSEL VID/PID 2E8A:000F and a nonempty serial. Returns `Ok(())` on admission
    /// or a diagnostic with all state cleared; does not inspect or write hardware.
    pub fn select(
        &mut self,
        device: DeviceSummary,
    ) -> Result<(), &'static str> {
        self.clear();
        if device.status != DeviceStatus::DescriptorOnly
            || device.vid != 0x2E8A
            || device.pid != 0x000F
            || device.serial.as_ref().is_none_or(|serial| serial.is_empty())
        {
            return Err("Select a uniquely identified RP2350 BOOTSEL device");
        }
        self.selected = Some(device);
        Ok(())
    }

    /// Validate `report` against `connection`'s session and the selected descriptor.
    /// Returns `Ok(())` after retaining it, or a diagnostic for invalid/missing/changed
    /// identity. Always clears prior inspection and acknowledgement before checking.
    pub fn accept(
        &mut self,
        connection: &Connection,
        report: DeviceInspection,
    ) -> Result<(), &'static str> {
        self.inspection = None;
        self.acknowledgement = None;
        report.validate().map_err(|_| "Invalid device inspection")?;
        let selected = self.selected.as_ref().ok_or("No device selected")?;
        if report.session_id != *connection.session()
            || report.device_id != selected.device_id
            || report.descriptor_sha256 != selected.descriptor_sha256
        {
            return Err("Device identity changed; scan and inspect again");
        }
        self.inspection = Some(report);
        Ok(())
    }

    /// Replace confirmation according to `confirmed`. False clears it successfully;
    /// true requires a complete inspection with information hash and returns an error
    /// otherwise. `Ok(())` records a user assertion, not measured flash capacity.
    pub fn acknowledge(
        &mut self,
        confirmed: bool,
    ) -> Result<(), &'static str> {
        self.acknowledgement = None;
        if confirmed {
            let report = self
                .inspection
                .as_ref()
                .filter(|r| r.complete)
                .ok_or("Inspect this device before confirming its board type")?;
            self.acknowledgement = Some(BoardAcknowledgement {
                device_id: report.device_id.clone(),
                session_id: report.session_id.clone(),
                information_sha256: report
                    .information_sha256
                    .clone()
                    .ok_or("Device inspection is incomplete")?,
                board: BoardContract::GenuinePico2W,
            });
        }
        Ok(())
    }
}
