//! Module for cursors over a byte slice.
//!

use bt_hci::WriteHci;

use crate::codec::{Decode, Encode, Error};

/// Not a byte writer. It is just a cursor to track where a byte slice is being written.
pub struct WriteCursor<'d> {
    pos: usize,
    data: &'d mut [u8],
}

impl<'d> WriteCursor<'d> {
    /// Creates a new write cursor at the beginning of the data.
    pub fn new(data: &'d mut [u8]) -> Self {
        Self { pos: 0, data }
    }

    /// Rewinds the cursor back to the beginning of the buffer.
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Moves the cursor back to the specified position from the start. This can only rewind the cursor.
    pub fn truncate(&mut self, npos: usize) {
        self.pos = self.pos.min(npos);
    }

    /// Split the remaining writable bytes into two cursors. The parent position
    /// is unchanged; commit the combined written length after both are dropped.
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

    /// Append byte slice
    pub fn append(&mut self, data: &[u8]) -> Result<(), Error> {
        if self.available() < data.len() {
            Err(Error::InsufficientSpace)
        } else {
            self.data[self.pos..self.pos + data.len()].copy_from_slice(data);
            self.pos += data.len();
            Ok(())
        }
    }

    /// Write fixed sized type
    pub fn write<E: Encode>(&mut self, data: E) -> Result<(), Error> {
        if self.available() < data.size() {
            Err(Error::InsufficientSpace)
        } else {
            data.encode(&mut self.data[self.pos..self.pos + data.size()])?;
            self.pos += data.size();
            Ok(())
        }
    }

    /// Write fixed sized type
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

    pub fn write_ref<E: Encode>(&mut self, data: &E) -> Result<(), Error> {
        if self.available() < data.size() {
            Err(Error::InsufficientSpace)
        } else {
            data.encode(&mut self.data[self.pos..self.pos + data.size()])?;
            self.pos += data.size();
            Ok(())
        }
    }

    /// Obtain a mutable slice of the remaining writable buffer, be sure to [commit] bytes written.
    pub fn write_buf(&mut self) -> &mut [u8] {
        &mut self.data[self.pos..]
    }

    /// Commits the specified length to the buffer, data may be written there through the [write_buf] method.
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
pub struct ReadCursor<'d> {
    pos: usize,
    data: &'d [u8],
}

impl<'d> ReadCursor<'d> {
    pub fn new(data: &'d [u8]) -> Self {
        Self { pos: 0, data }
    }

    pub fn read<T: Decode<'d>>(&mut self) -> Result<T, Error> {
        let src = &self.data[self.pos..];
        let val = T::decode(src)?;
        self.pos += val.size();
        Ok(val)
    }

    pub fn slice(&mut self, nbytes: usize) -> Result<&'d [u8], Error> {
        if self.available() < nbytes {
            Err(Error::InsufficientSpace)
        } else {
            let src = &self.data[self.pos..self.pos + nbytes];
            self.pos += nbytes;
            Ok(src)
        }
    }

    pub fn available(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn len(&self) -> usize {
        self.pos
    }

    pub fn remaining(self) -> &'d [u8] {
        &self.data[self.pos..]
    }

    pub fn consume(self, nbytes: usize) -> Result<&'d [u8], Error> {
        if self.available() < nbytes {
            Err(Error::InsufficientSpace)
        } else {
            Ok(&self.data[self.pos..self.pos + nbytes])
        }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
