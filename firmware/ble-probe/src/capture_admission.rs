//! Capture-format policy layered over the opaque journal. The caller exclusively
//! owns storage across inspection and save; no other journal owner may interleave.
use dashboard_core::discovery::names::SavedIdentity;
use dashboard_core::discovery::{capture, capture_store as journal, capture_ui as ui};

#[derive(Debug)]
/// Journal failure or capture-format policy preventing admission.
pub enum Error<E> {
    /// Physical storage, journal integrity, generation or slot-bound failure.
    Journal(
        /// Original journal error, including the storage-specific cause.
        journal::Error<E>,
    ),
    /// At least one persisted copy uses an unknown journal or capture version.
    Unsupported,
    /// A supported capture is malformed or an input payload fails decoding.
    Corrupt,
    /// A save expected an empty slot but inspection found retained data.
    Changed,
}
#[derive(Clone, Copy)]
/// Inspection result covering both journal copies while the caller retains storage ownership.
pub struct Admission {
    /// UI state distinguishing readable-but-blocked, unsupported, corrupt and empty slots.
    pub slot: ui::Slot,
    /// Newest supported header-valid copy, which can remain readable when writes are blocked.
    selected: Option<journal::StoredCapture>,
    /// Either copy has an unknown journal schema or capture header version.
    unsupported: bool,
    /// A journal-valid copy contains a malformed supported capture header.
    malformed: bool,
    /// Both physical copies are entirely empty according to the journal scan.
    empty: bool,
}
/// Inspects both copies of `slot` through exclusive `storage`. Returns readable
/// metadata plus replacement policy; it validates capture headers, not full bodies.
/// Unknown inactive copies also block replacement. Journal failures propagate;
/// generation exhaustion remains readable but blocked. Performs no writes.
pub async fn inspect<S: journal::Storage>(
    storage: &mut S,
    slot: u8,
) -> Result<Admission, Error<S::Error>> {
    let info = journal::inspect_slot(storage, slot).await.map_err(Error::Journal)?;
    let mut selected: Option<journal::StoredCapture> = None;
    let mut unsupported = false;
    let mut malformed = false;
    for copy in info.copies {
        match copy {
            journal::CopyState::UnsupportedSchema(_) => unsupported = true,
            journal::CopyState::Valid(meta) => {
                let mut prefix = [0; capture::HEADER_BYTES];
                let length = (meta.length as usize).min(prefix.len());
                journal::read_prefix(storage, meta, &mut prefix[..length])
                    .await
                    .map_err(Error::Journal)?;
                match capture::check_header(&prefix[..length], meta.length as usize) {
                    Ok(()) => {
                        if selected.is_none_or(|old| meta.generation > old.generation) {
                            selected = Some(meta);
                        }
                    }
                    Err(capture::CaptureError::Version) => unsupported = true,
                    Err(_) => malformed = true,
                }
            }
            _ => {}
        }
    }
    let empty = info.copies == [journal::CopyState::Empty; 2];
    let slot = match selected {
        Some(meta) if unsupported || malformed || info.selected.is_some_and(|m| m.generation == u64::MAX) => {
            ui::Slot::SavedBlocked {
                generation: meta.generation,
            }
        }
        Some(meta) => {
            ui::Slot::Saved {
                generation: meta.generation,
            }
        }
        None if unsupported => ui::Slot::Unsupported,
        None if empty => ui::Slot::Empty,
        None => ui::Slot::Corrupt,
    };
    Ok(Admission {
        slot,
        selected,
        unsupported,
        malformed,
        empty,
    })
}

/// Selects the newest supported header-valid copy in `slot` and reads it into
/// `destination` via `storage`. Returns its metadata or empty/version/corruption/
/// journal failure. The caller must decode the full returned payload before use;
/// unknown sibling copies do not hide a readable supported copy.
pub async fn load<S: journal::Storage>(
    storage: &mut S,
    slot: u8,
    destination: &mut [u8],
) -> Result<journal::StoredCapture, Error<S::Error>> {
    let admitted = inspect(storage, slot).await?;
    let selected = admitted.selected.ok_or(if admitted.unsupported {
        Error::Unsupported
    } else if admitted.empty {
        Error::Journal(journal::Error::Empty)
    } else {
        Error::Corrupt
    })?;
    journal::load_copy(storage, selected, destination)
        .await
        .map_err(Error::Journal)
}

/// Derive a directory label only after loading and fully decoding the selected
/// supported copy. The caller lends its existing payload buffer exclusively.
/// `storage` scans `slot`, `bytes` receives its selected payload, and `decoder`
/// validates the complete capture. Returns slot state and optional saved identity;
/// unsupported/corrupt data never supplies a label. Empty/unreadable selections
/// return their state with no identity; load/decode errors propagate.
pub async fn directory_entry<S: journal::Storage>(
    storage: &mut S,
    decoder: &mut capture::Decoder,
    slot: u8,
    bytes: &mut [u8],
) -> Result<(ui::Slot, Option<SavedIdentity>), Error<S::Error>> {
    let admitted = inspect(storage, slot).await?;
    let Some(selected) = admitted.selected else {
        return Ok((admitted.slot, None));
    };
    journal::load_copy(storage, selected, bytes)
        .await
        .map_err(Error::Journal)?;
    let view = decoder.decode(&bytes[..selected.length as usize]).map_err(|error| {
        if error == capture::CaptureError::Version {
            Error::Unsupported
        } else {
            Error::Corrupt
        }
    })?;
    Ok((admitted.slot, Some(SavedIdentity::from_capture(&view))))
}

/// Reinspects both copies of `slot` in `storage`, fully validates `payload` with
/// `decoder`, then returns metadata for the committed/readback-verified copy.
/// `overwrite` permits replacing nonempty known data, never unknown formats;
/// false rejects a slot that became nonempty. Journal failures can leave a torn
/// inactive copy while preserving the prior committed copy. Never cancel mutation.
pub async fn save<S: journal::Storage>(
    storage: &mut S,
    decoder: &mut capture::Decoder,
    slot: u8,
    payload: &[u8],
    overwrite: bool,
) -> Result<journal::StoredCapture, Error<S::Error>> {
    // Recheck both persisted copies immediately before entering the journal's
    // mutation path, including the inactive copy that the journal would erase.
    let admitted = inspect(storage, slot).await?;
    if admitted.unsupported {
        return Err(Error::Unsupported);
    }
    if admitted.malformed {
        return Err(Error::Corrupt);
    }
    if !overwrite && !admitted.empty {
        return Err(Error::Changed);
    }
    decoder.decode(payload).map_err(|_| Error::Corrupt)?;
    journal::save_slot(storage, slot, payload).await.map_err(Error::Journal)
}
