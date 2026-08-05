use crate::{Frame, FrameInfo, MediaError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PictureId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpbEntry {
    pub id: PictureId,
    pub order: i64,
    pub frame: Frame,
    pub reference: bool,
    pub keyframe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPictureBuffer {
    info: FrameInfo,
    capacity: usize,
    entries: Vec<DpbEntry>,
}

impl DpbEntry {
    pub fn new(id: PictureId, order: i64, frame: Frame) -> Self {
        Self {
            id,
            order,
            frame,
            reference: true,
            keyframe: false,
        }
    }

    pub fn with_reference(mut self, reference: bool) -> Self {
        self.reference = reference;
        self
    }

    pub fn with_keyframe(mut self, keyframe: bool) -> Self {
        self.keyframe = keyframe;
        self
    }
}

impl DecodedPictureBuffer {
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

    pub fn info(&self) -> FrameInfo {
        self.info
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.entries.len() >= self.capacity
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &DpbEntry> {
        self.entries.iter()
    }

    pub fn references(&self) -> impl DoubleEndedIterator<Item = &DpbEntry> {
        self.entries.iter().filter(|entry| entry.reference)
    }

    pub fn get(&self, id: PictureId) -> Option<&DpbEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn latest_reference(&self) -> Option<&DpbEntry> {
        self.entries.iter().rev().find(|entry| entry.reference)
    }

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

    pub fn set_reference(&mut self, id: PictureId, reference: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.reference = reference;
        true
    }

    pub fn evict_oldest_non_reference(&mut self) -> Option<DpbEntry> {
        let index = self.entries.iter().position(|entry| !entry.reference)?;
        Some(self.entries.remove(index))
    }

    pub fn evict_oldest(&mut self) -> Option<DpbEntry> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.remove(0))
        }
    }

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
