//! Module for cursors over a byte slice.
//!

use bt_hci::WriteHci;

use crate::codec::{Decode, Encode, Error};

/// Not a byte writer. It is just a cursor to track where a byte slice is being written.
pub struct WriteCursor<'d> {
    /// Current committed/read byte position, bounded by backing slice length.
    pos: usize,
    /// Exclusive destination storage; uncommitted bytes may contain previous data.
    data: &'d mut [u8],
}

impl<'d> WriteCursor<'d> {
    /// Returns a cursor at offset zero over exclusive `data`, without clearing bytes.
    pub fn new(data: &'d mut [u8]) -> Self {
        Self { pos: 0, data }
    }

    /// Resets the committed position to zero without clearing backing bytes.
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Rewinds to absolute byte `npos`, clamping forward requests to the current position.
    pub fn truncate(&mut self, npos: usize) {
        self.pos = self.pos.min(npos);
    }

    /// Split the remaining writable bytes into two cursors. The parent position
    /// is unchanged; commit the combined written length after both are dropped.
    /// Returns cursors for the next `nbytes` and remaining tail, or InsufficientSpace without change.
    pub fn split(&mut self, nbytes: usize) -> Result<(WriteCursor<'_>, WriteCursor<'_>), Error> {
        if self.available() < nbytes {
            Err(Error::InsufficientSpace)
        } else {
            let (first, second) = self.data[self.pos..].split_at_mut(nbytes);
            Ok((
                WriteCursor { data: first, pos: 0 },
                WriteCursor { pos: 0, data: second },
            ))
        }
    }

    /// Appends `data` and advances on success; returns InsufficientSpace without writing if it cannot fit.
    pub fn append(&mut self, data: &[u8]) -> Result<(), Error> {
        if self.available() < data.len() {
            Err(Error::InsufficientSpace)
        } else {
            self.data[self.pos..self.pos + data.len()].copy_from_slice(data);
            self.pos += data.len();
            Ok(())
        }
    }

    /// Encodes owned `data` into exactly its reported size and returns success after advancing.
    /// Short capacity does not write; encoder errors retain position but may have modified bytes.
    pub fn write<E: Encode>(&mut self, data: E) -> Result<(), Error> {
        if self.available() < data.size() {
            Err(Error::InsufficientSpace)
        } else {
            data.encode(&mut self.data[self.pos..self.pos + data.size()])?;
            self.pos += data.size();
            Ok(())
        }
    }

    /// Serializes borrowed HCI `data` and returns success after advancing by its reported size.
    /// Any HCI write failure maps to InsufficientSpace; bytes can change while position stays fixed.
    pub fn write_hci<E: WriteHci>(&mut self, data: &E) -> Result<(), Error> {
        if self.available() < data.size() {
            Err(Error::InsufficientSpace)
        } else {
            data.write_hci(&mut self.data[self.pos..self.pos + data.size()])
                .map_err(|_| Error::InsufficientSpace)?;
            self.pos += data.size();
            Ok(())
        }
    }

    /// Encodes borrowed `data` and returns success after advancing by its size.
    /// Capacity/codec failure leaves position unchanged; codec failure may leave partial bytes.
    pub fn write_ref<E: Encode>(&mut self, data: &E) -> Result<(), Error> {
        if self.available() < data.size() {
            Err(Error::InsufficientSpace)
        } else {
            data.encode(&mut self.data[self.pos..self.pos + data.size()])?;
            self.pos += data.size();
            Ok(())
        }
    }

    /// Returns uncommitted writable bytes; call [`Self::commit`] to advance after writing.
    pub fn write_buf(&mut self) -> &mut [u8] {
        &mut self.data[self.pos..]
    }

    /// Advances by `len` bytes written through [`Self::write_buf`], without validating their contents.
    /// Returns InsufficientSpace without advancing when the requested count exceeds remaining capacity.
    pub fn commit(&mut self, len: usize) -> Result<(), Error> {
        if self.available() < len {
            Err(Error::InsufficientSpace)
        } else {
            self.pos += len;
            Ok(())
        }
    }

    /// Returns amount of bytes that remain available.
    pub fn available(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Returns the current length of the data written.
    pub fn len(&self) -> usize {
        self.pos
    }

    /// Returns the byte slice that was written by this cursor.
    pub fn finish(self) -> &'d mut [u8] {
        &mut self.data[..self.pos]
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug)]
/// Borrowed source cursor; decoders must report a consumed size within their supplied input.
pub struct ReadCursor<'d> {
    /// Current committed/read byte position, bounded by backing slice length.
    pos: usize,
    /// Immutable source storage retained for borrowed decoded values.
    data: &'d [u8],
}

impl<'d> ReadCursor<'d> {
    /// Returns a cursor at offset zero over `data`.
    pub fn new(data: &'d [u8]) -> Self {
        Self { pos: 0, data }
    }

    /// Returns the next decoded value and advances by its reported size; codec error retains position.
    /// A decoder violating its size contract can make later cursor operations panic.
    pub fn read<T: Decode<'d>>(&mut self) -> Result<T, Error> {
        let src = &self.data[self.pos..];
        let val = T::decode(src)?;
        self.pos += val.size();
        Ok(val)
    }

    /// Returns the next `nbytes` and advances, or InsufficientSpace without changing position.
    pub fn slice(&mut self, nbytes: usize) -> Result<&'d [u8], Error> {
        if self.available() < nbytes {
            Err(Error::InsufficientSpace)
        } else {
            let src = &self.data[self.pos..self.pos + nbytes];
            self.pos += nbytes;
            Ok(src)
        }
    }

    /// Returns unread byte count, assuming the decoder size contract has held.
    pub fn available(&self) -> usize {
        self.data.len() - self.pos
    }

    /// Returns the number of bytes consumed since creation/reset.
    pub fn len(&self) -> usize {
        self.pos
    }

    /// Consumes the cursor and returns the unread suffix, including an empty suffix at end.
    pub fn remaining(self) -> &'d [u8] {
        &self.data[self.pos..]
    }

    /// Consumes the cursor and returns only the next `nbytes`, or InsufficientSpace.
    /// Any remaining tail is not included in the returned slice.
    pub fn consume(self, nbytes: usize) -> Result<&'d [u8], Error> {
        if self.available() < nbytes {
            Err(Error::InsufficientSpace)
        } else {
            Ok(&self.data[self.pos..self.pos + nbytes])
        }
    }

    /// Rewinds to the original source start; previously returned borrows remain valid.
    pub fn reset(&mut self) {
        self.pos = 0;
    }
}

#[cfg(test)]
/// Regression coverage for split borrowing, capacity admission and explicit parent commit.
mod tests {
    use super::*;

    #[test]
    /// Split writes preserve the old prefix and require explicit combined parent commit.
    fn split_preserves_written_prefix_and_parent_position() {
        let mut bytes = [0; 6];
        let mut cursor = WriteCursor::new(&mut bytes);
        cursor.append(&[0xaa, 0xbb]).unwrap();
        let (mut header, mut body) = cursor.split(1).unwrap();
        header.append(&[0xcc]).unwrap();
        body.append(&[1, 2, 3]).unwrap();
        assert_eq!(cursor.len(), 2);
        cursor.commit(4).unwrap();
        assert_eq!(cursor.finish(), [0xaa, 0xbb, 0xcc, 1, 2, 3]);
    }

    #[test]
    /// Oversized and empty splits preserve the bounded parent cursor.
    fn split_checks_remaining_capacity_without_changing_cursor() {
        let mut bytes = [0; 3];
        let mut cursor = WriteCursor::new(&mut bytes);
        cursor.append(&[7, 8]).unwrap();
        assert!(matches!(cursor.split(2), Err(Error::InsufficientSpace)));
        assert_eq!(cursor.len(), 2);
        let (first, second) = cursor.split(0).unwrap();
        assert_eq!(first.available(), 0);
        assert_eq!(second.available(), 1);
        cursor.commit(1).unwrap();
        let (first, second) = cursor.split(0).unwrap();
        assert_eq!(first.available(), 0);
        assert_eq!(second.available(), 0);
    }
}
