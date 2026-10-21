//! Journal/UI integration preserves readable supported evidence beside an unsupported sibling until explicit full
//! reset.

use dashboard_core::discovery::{capture_store as journal, capture_ui as ui};
use embassy_futures::block_on;

/// In-memory NOR model for the reserved four-slot range.
struct Flash(
    /// Bytes addressed relative to FLASH_OFFSET, initialized erased by the test.
    Vec<u8>,
);
impl journal::Storage for Flash {
    /// No recoverable I/O failures are injected; invalid ranges and zero-to-one programming panic.
    type Error = ();

    /// Copies reserved-range `offset` bytes into `bytes`; returns success, panicking on underflow/out-of-bounds ranges.
    async fn read(
        &mut self,
        offset: u32,
        bytes: &mut [u8],
    ) -> Result<(), ()> {
        let at = (offset - journal::FLASH_OFFSET) as usize;
        bytes.copy_from_slice(&self.0[at..at + bytes.len()]);
        Ok(())
    }

    /// Fills one sector at `offset` with 0xFF and returns success; invalid ranges panic and alignment is caller-owned.
    async fn erase_sector(
        &mut self,
        offset: u32,
    ) -> Result<(), ()> {
        let at = (offset - journal::FLASH_OFFSET) as usize;
        self.0[at..at + journal::SECTOR_BYTES].fill(255);
        Ok(())
    }

    /// AND-programs `bytes` at `offset` and returns success; zero-to-one transitions/range violations panic.
    /// Earlier byte effects remain if a later assertion fails; page alignment is caller-owned.
    async fn program_page(
        &mut self,
        offset: u32,
        bytes: &[u8; journal::PAGE_BYTES],
    ) -> Result<(), ()> {
        let at = (offset - journal::FLASH_OFFSET) as usize;
        for (target, source) in self.0[at..at + bytes.len()].iter_mut().zip(bytes) {
            assert_eq!(*target & source, *source);
            *target &= source;
        }
        Ok(())
    }
}

/// Unknown sibling schema blocks overwrite but permits loading the older supported copy; only confirmed Reset erases
/// both.
#[test]
fn older_valid_capture_remains_loadable_and_unknown_sibling_requires_full_reset_confirmation() {
    block_on(async {
        let mut flash = Flash(vec![255; journal::SLOT_BYTES * 4]);
        journal::save_slot(&mut flash, 0, b"older supported capture")
            .await
            .unwrap();
        journal::save_slot(&mut flash, 0, b"newer unsupported capture")
            .await
            .unwrap();
        flash.0[journal::COPY_BYTES + 8..journal::COPY_BYTES + 10].copy_from_slice(&2u16.to_le_bytes());
        let info = journal::inspect_slot(&mut flash, 0).await.unwrap();
        assert_eq!(info.selected.unwrap().generation, 1);
        assert_eq!(info.copies[1], journal::CopyState::UnsupportedSchema(2));
        let status = ui::slot_from_journal(&info);
        assert_eq!(status, ui::Slot::SavedBlocked { generation: 1 });
        assert_eq!(
            journal::save_slot(&mut flash, 0, b"replacement").await,
            Err(journal::Error::UnsupportedSchema(2))
        );
        let mut state = ui::CaptureUi::new();
        let context = ui::Context {
            save: ui::SaveAvailability::Ready,
            revision: 1,
        };
        state.input(ui::Input::XDown, 0, context);
        let ui::Action::Refresh { id } = state.tick(2000).unwrap() else {
            panic!()
        };
        state.complete(
            id,
            ui::Completion::Listed {
                slots: [status, ui::Slot::Empty, ui::Slot::Empty, ui::Slot::Empty],
                identities: [None; 4],
            },
        );
        state.input(ui::Input::XUp, 2001, context);
        let ui::Action::Load { id, slot: 0 } = state.input(ui::Input::Click, 2100, context).unwrap() else {
            panic!()
        };
        let mut output = [0; 128];
        let loaded = journal::load_slot(&mut flash, 0, &mut output).await.unwrap();
        assert_eq!(&output[..loaded.length as usize], b"older supported capture");
        state.complete(id, ui::Completion::Loaded { pages: 1 });
        state.input(ui::Input::Back, 2200, context);
        state.input(ui::Input::XDown, 2300, context);
        state.input(ui::Input::XUp, 2301, context);
        assert_eq!(state.screen(), ui::Screen::ConfirmReset);
        state.input(ui::Input::Back, 2302, context);
        assert_eq!(journal::inspect_slot(&mut flash, 0).await.unwrap(), info);
        state.input(ui::Input::XDown, 2400, context);
        state.input(ui::Input::XUp, 2401, context);
        let ui::Action::Reset { id, slot: 0 } = state.input(ui::Input::Click, 2402, context).unwrap() else {
            panic!()
        };
        journal::reset_slot(&mut flash, 0).await.unwrap();
        state.complete(id, ui::Completion::Reset);
        assert_eq!(state.slots()[0], ui::Slot::Empty);
        assert!(
            journal::inspect_slot(&mut flash, 0)
                .await
                .unwrap()
                .copies
                .iter()
                .all(|v| *v == journal::CopyState::Empty)
        );
    });
}
