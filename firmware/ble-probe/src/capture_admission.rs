//! Capture-format policy layered over the opaque journal. The caller exclusively
//! owns storage across inspection and save; no other journal owner may interleave.
use dashboard_core::discovery::names::SavedIdentity;
use dashboard_core::discovery::{capture, capture_store as journal, capture_ui as ui};

#[derive(Debug)]
pub enum Error<E> {
    Journal(journal::Error<E>),
    Unsupported,
    Corrupt,
    Changed,
}
#[derive(Clone, Copy)]
pub struct Admission {
    pub slot: ui::Slot,
    selected: Option<journal::StoredCapture>,
    unsupported: bool,
    malformed: bool,
    empty: bool,
}
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
