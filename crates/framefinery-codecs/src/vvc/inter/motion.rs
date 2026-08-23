#![cfg_attr(not(test), allow(dead_code))]

use super::VvcSampledFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionVector {
    pub(in crate::vvc) x: i32,
    pub(in crate::vvc) y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionCandidate {
    pub(in crate::vvc) ref_origin_x: usize,
    pub(in crate::vvc) ref_origin_y: usize,
    pub(in crate::vvc) mv: VvcLumaMotionVector,
    pub(in crate::vvc) sad: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionSearchBlock {
    pub(in crate::vvc) origin_x: usize,
    pub(in crate::vvc) origin_y: usize,
    pub(in crate::vvc) width: usize,
    pub(in crate::vvc) height: usize,
}

pub(in crate::vvc) fn vvc_luma_diamond_motion_search(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    block: VvcLumaMotionSearchBlock,
    predictor_mvs: &[VvcLumaMotionVector],
    search_radius: usize,
) -> Option<VvcLumaMotionCandidate> {
    if current.geometry != reference.geometry || current.format != reference.format {
        return None;
    }
    if block.width == 0 || block.height == 0 {
        return None;
    }
    if block.origin_x.checked_add(block.width)? > current.geometry.width
        || block.origin_y.checked_add(block.height)? > current.geometry.height
    {
        return None;
    }
    if block.width > reference.geometry.width || block.height > reference.geometry.height {
        return None;
    }

    let mut best =
        vvc_luma_motion_candidate_at(current, reference, block, block.origin_x, block.origin_y)?;
    for predictor in predictor_mvs {
        if predictor.x.unsigned_abs() as usize > search_radius
            || predictor.y.unsigned_abs() as usize > search_radius
        {
            continue;
        }
        let Some(ref_origin_x) = offset_origin(block.origin_x, predictor.x) else {
            continue;
        };
        let Some(ref_origin_y) = offset_origin(block.origin_y, predictor.y) else {
            continue;
        };
        if let Some(candidate) =
            vvc_luma_motion_candidate_at(current, reference, block, ref_origin_x, ref_origin_y)
        {
            if candidate.is_better_than(best) {
                best = candidate;
            }
        }
    }

    loop {
        let mut improved = false;
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next_mv = VvcLumaMotionVector {
                x: best.mv.x + dx,
                y: best.mv.y + dy,
            };
            if next_mv.x.unsigned_abs() as usize > search_radius
                || next_mv.y.unsigned_abs() as usize > search_radius
            {
                continue;
            }
            let Some(ref_origin_x) = offset_origin(block.origin_x, next_mv.x) else {
                continue;
            };
            let Some(ref_origin_y) = offset_origin(block.origin_y, next_mv.y) else {
                continue;
            };
            let Some(candidate) =
                vvc_luma_motion_candidate_at(current, reference, block, ref_origin_x, ref_origin_y)
            else {
                continue;
            };
            if candidate.is_better_than(best) {
                best = candidate;
                improved = true;
                break;
            }
        }
        if !improved {
            return Some(best);
        }
    }
}

impl VvcLumaMotionCandidate {
    fn is_better_than(self, other: Self) -> bool {
        if self.sad != other.sad {
            return self.sad < other.sad;
        }
        let self_mv_cost = motion_vector_tie_cost(self.mv);
        let other_mv_cost = motion_vector_tie_cost(other.mv);
        if self_mv_cost != other_mv_cost {
            return self_mv_cost < other_mv_cost;
        }
        (self.mv.y, self.mv.x) < (other.mv.y, other.mv.x)
    }
}

fn motion_vector_tie_cost(mv: VvcLumaMotionVector) -> u64 {
    u64::from(mv.x.unsigned_abs()) + u64::from(mv.y.unsigned_abs())
}

fn vvc_luma_motion_candidate_at(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    block: VvcLumaMotionSearchBlock,
    ref_origin_x: usize,
    ref_origin_y: usize,
) -> Option<VvcLumaMotionCandidate> {
    if ref_origin_x.checked_add(block.width)? > reference.geometry.width
        || ref_origin_y.checked_add(block.height)? > reference.geometry.height
    {
        return None;
    }
    let mv_x = ref_origin_x as i64 - block.origin_x as i64;
    let mv_y = ref_origin_y as i64 - block.origin_y as i64;
    let mv = VvcLumaMotionVector {
        x: i32::try_from(mv_x).ok()?,
        y: i32::try_from(mv_y).ok()?,
    };
    Some(VvcLumaMotionCandidate {
        ref_origin_x,
        ref_origin_y,
        mv,
        sad: vvc_luma_block_sad(current, reference, block, ref_origin_x, ref_origin_y),
    })
}

fn vvc_luma_block_sad(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    block: VvcLumaMotionSearchBlock,
    ref_origin_x: usize,
    ref_origin_y: usize,
) -> u64 {
    let current_stride = current.geometry.width;
    let reference_stride = reference.geometry.width;
    let mut sad = 0u64;
    for y in 0..block.height {
        let current_row = (block.origin_y + y) * current_stride + block.origin_x;
        let reference_row = (ref_origin_y + y) * reference_stride + ref_origin_x;
        for x in 0..block.width {
            let current_sample = current.luma[current_row + x];
            let reference_sample = reference.luma[reference_row + x];
            sad += u64::from(current_sample.abs_diff(reference_sample));
        }
    }
    sad
}

fn offset_origin(origin: usize, delta: i32) -> Option<usize> {
    if delta >= 0 {
        origin.checked_add(delta as usize)
    } else {
        origin.checked_sub(delta.unsigned_abs() as usize)
    }
}

#[cfg(test)]
mod tests {
    use crate::picture::{ChromaSampling, SampleBitDepth};

    use super::*;
    use crate::vvc::{VvcPictureFormat, VvcSample, VvcVideoGeometry};

    #[test]
    fn vvc_luma_diamond_motion_search_finds_shifted_block() {
        let mut reference = motion_test_frame(16, 16);
        for y in 0..8 {
            for x in 0..8 {
                reference.luma[(3 + y) * 16 + 2 + x] = (600 + y * 17 + x * 3) as VvcSample;
            }
        }
        let mut current = motion_test_frame(16, 16);
        for y in 0..8 {
            for x in 0..8 {
                current.luma[(4 + y) * 16 + 6 + x] = reference.luma[(3 + y) * 16 + 2 + x];
            }
        }

        let candidate = vvc_luma_diamond_motion_search(
            &current,
            &reference,
            VvcLumaMotionSearchBlock {
                origin_x: 6,
                origin_y: 4,
                width: 8,
                height: 8,
            },
            &[VvcLumaMotionVector { x: -4, y: -1 }],
            6,
        )
        .expect("search should find a valid candidate");

        assert_eq!(candidate.ref_origin_x, 2);
        assert_eq!(candidate.ref_origin_y, 3);
        assert_eq!(candidate.mv, VvcLumaMotionVector { x: -4, y: -1 });
        assert_eq!(candidate.sad, 0);
    }

    #[test]
    fn vvc_luma_diamond_motion_search_prefers_zero_mv_on_ties() {
        let current = constant_motion_test_frame(16, 16, 77);
        let reference = constant_motion_test_frame(16, 16, 77);

        let candidate = vvc_luma_diamond_motion_search(
            &current,
            &reference,
            VvcLumaMotionSearchBlock {
                origin_x: 4,
                origin_y: 4,
                width: 8,
                height: 8,
            },
            &[VvcLumaMotionVector { x: -3, y: 0 }],
            4,
        )
        .expect("search should find a valid candidate");

        assert_eq!(candidate.ref_origin_x, 4);
        assert_eq!(candidate.ref_origin_y, 4);
        assert_eq!(candidate.mv, VvcLumaMotionVector { x: 0, y: 0 });
        assert_eq!(candidate.sad, 0);
    }

    #[test]
    fn vvc_luma_diamond_motion_search_rejects_invalid_blocks() {
        let current = motion_test_frame(16, 16);
        let reference = motion_test_frame(16, 16);

        assert_eq!(
            vvc_luma_diamond_motion_search(
                &current,
                &reference,
                VvcLumaMotionSearchBlock {
                    origin_x: 12,
                    origin_y: 12,
                    width: 8,
                    height: 8,
                },
                &[],
                4,
            ),
            None
        );
    }

    fn motion_test_frame(width: usize, height: usize) -> VvcSampledFrame {
        let mut frame = constant_motion_test_frame(width, height, 0);
        for y in 0..height {
            for x in 0..width {
                frame.luma[y * width + x] = ((y * 257 + x * 19 + 11) & 0x0fff) as VvcSample;
            }
        }
        frame
    }

    fn constant_motion_test_frame(
        width: usize,
        height: usize,
        value: VvcSample,
    ) -> VvcSampledFrame {
        let geometry = VvcVideoGeometry { width, height };
        let chroma_len = (width / 2) * (height / 2);
        VvcSampledFrame {
            geometry,
            format: VvcPictureFormat {
                chroma_sampling: ChromaSampling::Cs420,
                bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
            },
            luma: vec![value; width * height],
            cb: vec![512; chroma_len],
            cr: vec![512; chroma_len],
            chroma_len,
        }
    }
}
