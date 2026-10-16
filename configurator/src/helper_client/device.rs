//! Device confirmation is a user choice bound to a single inspection snapshot.
use dashboard_helper_protocol::*;

use super::Connection;

#[derive(Clone, Default)]
pub struct DeviceSelection {
    selected: Option<DeviceSummary>,
    inspection: Option<DeviceInspection>,
    acknowledgement: Option<BoardAcknowledgement>,
}
impl DeviceSelection {
    pub fn selected(&self) -> Option<&DeviceSummary> { self.selected.as_ref() }

    pub fn inspection(&self) -> Option<&DeviceInspection> { self.inspection.as_ref() }

    pub fn acknowledgement(&self) -> Option<&BoardAcknowledgement> { self.acknowledgement.as_ref() }

    pub fn clear(&mut self) { *self = Self::default(); }

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
                board: BoardContract::GenuinePico2,
            });
        }
        Ok(())
    }
}
