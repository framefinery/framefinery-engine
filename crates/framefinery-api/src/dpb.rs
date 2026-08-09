use crate::{Frame, FrameInfo, MediaError, Result};

/// Stable identifier for a picture stored in a decoded picture buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PictureId(pub u64);

/// One decoded or reconstructed picture tracked by a decoded picture buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpbEntry {
    /// Caller-owned picture identifier.
    pub id: PictureId,
    /// Display or codec-order value used for ordering decisions.
    pub order: i64,
    /// Decoded or reconstructed frame data.
    pub frame: Frame,
    /// Whether this picture may be used as a future reference.
    pub reference: bool,
    /// Whether this picture is a keyframe or random-access anchor.
    pub keyframe: bool,
}

/// Small validated decoded picture buffer for codec reference-frame storage.
///
/// The buffer enforces one [`FrameInfo`] for every stored frame and leaves
/// codec-specific reference-list policy to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPictureBuffer {
    info: FrameInfo,
    capacity: usize,
    entries: Vec<DpbEntry>,
}

impl DpbEntry {
    /// Create a reference picture entry with `keyframe` set to `false`.
    pub fn new(id: PictureId, order: i64, frame: Frame) -> Self {
        Self {
            id,
            order,
            frame,
            reference: true,
            keyframe: false,
        }
    }

    /// Set whether this entry is eligible as a reference picture.
    pub fn with_reference(mut self, reference: bool) -> Self {
        self.reference = reference;
        self
    }

    /// Set whether this entry represents a keyframe.
    pub fn with_keyframe(mut self, keyframe: bool) -> Self {
        self.keyframe = keyframe;
        self
    }
}

impl DecodedPictureBuffer {
    /// Create an empty decoded picture buffer for `info`.
    ///
    /// Returns an error when `capacity` is zero.
    pub fn new(info: FrameInfo, capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(MediaError::Message(
                "decoded picture buffer capacity must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            info,
            capacity,
            entries: Vec::new(),
        })
    }

    /// Frame metadata required for every entry in the buffer.
    pub fn info(&self) -> FrameInfo {
        self.info
    }

    /// Maximum number of entries retained by the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Current number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return whether the buffer contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return whether the buffer has reached its configured capacity.
    pub fn is_full(&self) -> bool {
        self.entries.len() >= self.capacity
    }

    /// Iterate over entries in insertion order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &DpbEntry> {
        self.entries.iter()
    }

    /// Iterate over entries currently marked as references.
    pub fn references(&self) -> impl DoubleEndedIterator<Item = &DpbEntry> {
        self.entries.iter().filter(|entry| entry.reference)
    }

    /// Return the entry with the requested picture id, if it is present.
    pub fn get(&self, id: PictureId) -> Option<&DpbEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Return the newest entry currently marked as a reference.
    pub fn latest_reference(&self) -> Option<&DpbEntry> {
        self.entries.iter().rev().find(|entry| entry.reference)
    }

    /// Insert an entry without eviction.
    ///
    /// Returns an error when the frame metadata differs, the picture id is
    /// already present, or the buffer is full.
    pub fn insert(&mut self, entry: DpbEntry) -> Result<()> {
        self.validate_entry(&entry)?;
        if self.is_full() {
            return Err(MediaError::Message(format!(
                "decoded picture buffer capacity {} is full",
                self.capacity
            )));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Insert an entry, evicting one old entry first when the buffer is full.
    ///
    /// Non-reference entries are evicted before reference entries. The evicted
    /// entry is returned when an eviction happened.
    pub fn insert_evicting_oldest(&mut self, entry: DpbEntry) -> Result<Option<DpbEntry>> {
        self.validate_entry(&entry)?;
        let evicted = if self.is_full() {
            self.evict_oldest_non_reference()
                .or_else(|| self.evict_oldest())
        } else {
            None
        };
        self.entries.push(entry);
        Ok(evicted)
    }

    /// Update the reference flag for an existing entry.
    ///
    /// Returns `true` when the picture id was found.
    pub fn set_reference(&mut self, id: PictureId, reference: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.reference = reference;
        true
    }

    /// Remove and return the oldest non-reference entry.
    pub fn evict_oldest_non_reference(&mut self) -> Option<DpbEntry> {
        let index = self.entries.iter().position(|entry| !entry.reference)?;
        Some(self.entries.remove(index))
    }

    /// Remove and return the oldest entry regardless of reference state.
    pub fn evict_oldest(&mut self) -> Option<DpbEntry> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.remove(0))
        }
    }

    /// Remove every entry from the buffer.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn validate_entry(&self, entry: &DpbEntry) -> Result<()> {
        if entry.frame.info() != self.info {
            return Err(MediaError::IncompatibleFormat {
                format: entry.frame.info().format.name(),
                reason: format!(
                    "decoded picture buffer expects {}x{}:{}",
                    self.info.width, self.info.height, self.info.format
                ),
            });
        }
        if self.entries.iter().any(|current| current.id == entry.id) {
            return Err(MediaError::Message(format!(
                "decoded picture buffer already contains picture id {}",
                entry.id.0
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PixelFormat;

    fn frame(info: FrameInfo, fill: u8) -> Frame {
        Frame::new(info, vec![fill; info.expected_len()]).unwrap()
    }

    #[test]
    fn inserts_and_finds_latest_reference() {
        let info = FrameInfo::new(4, 4, PixelFormat::Rgb24).unwrap();
        let mut dpb = DecodedPictureBuffer::new(info, 3).unwrap();

        dpb.insert(DpbEntry::new(PictureId(1), 0, frame(info, 1)))
            .unwrap();
        dpb.insert(DpbEntry::new(PictureId(2), 1, frame(info, 2)).with_reference(false))
            .unwrap();
        dpb.insert(DpbEntry::new(PictureId(3), 2, frame(info, 3)))
            .unwrap();

        assert_eq!(dpb.len(), 3);
        assert_eq!(
            dpb.latest_reference().map(|entry| entry.id),
            Some(PictureId(3))
        );
        assert!(dpb.set_reference(PictureId(3), false));
        assert_eq!(
            dpb.latest_reference().map(|entry| entry.id),
            Some(PictureId(1))
        );
    }

    #[test]
    fn evicts_oldest_non_reference_before_reference() {
        let info = FrameInfo::new(4, 4, PixelFormat::Rgb24).unwrap();
        let mut dpb = DecodedPictureBuffer::new(info, 2).unwrap();

        dpb.insert(DpbEntry::new(PictureId(1), 0, frame(info, 1)))
            .unwrap();
        dpb.insert(DpbEntry::new(PictureId(2), 1, frame(info, 2)).with_reference(false))
            .unwrap();
        let evicted = dpb
            .insert_evicting_oldest(DpbEntry::new(PictureId(3), 2, frame(info, 3)))
            .unwrap()
            .expect("one entry should be evicted");

        assert_eq!(evicted.id, PictureId(2));
        assert!(dpb.get(PictureId(1)).is_some());
        assert!(dpb.get(PictureId(3)).is_some());
    }

    #[test]
    fn rejects_mismatched_frame_info() {
        let info = FrameInfo::new(4, 4, PixelFormat::Rgb24).unwrap();
        let other = FrameInfo::new(8, 4, PixelFormat::Rgb24).unwrap();
        let mut dpb = DecodedPictureBuffer::new(info, 1).unwrap();
        let err = dpb
            .insert(DpbEntry::new(PictureId(1), 0, frame(other, 1)))
            .unwrap_err();

        assert!(err.to_string().contains("decoded picture buffer expects"));
    }
}
