#![cfg_attr(not(feature = "dead-code-audit"), allow(dead_code))]

use super::{VvcCtuRegion, VvcSampledFrame};

const VVC_LUMA_MOTION_BLOCK: usize = 8;
const VVC_LUMA_EXACT_MOTION_EARLY_EXIT_MAX_TIE_COST: u64 = 8;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionRegionAnalysis {
    pub(in crate::vvc) block_count: usize,
    pub(in crate::vvc) exact_count: usize,
    pub(in crate::vvc) nonzero_count: usize,
    pub(in crate::vvc) nonzero_exact_count: usize,
    pub(in crate::vvc) near_count: usize,
    pub(in crate::vvc) nonzero_near_count: usize,
    pub(in crate::vvc) total_sad: u64,
    pub(in crate::vvc) aggregate_16x16: VvcLumaMotionAggregateSummary,
    pub(in crate::vvc) aggregate_32x32: VvcLumaMotionAggregateSummary,
    pub(in crate::vvc) aggregate_64x64: VvcLumaMotionAggregateSummary,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionAggregateSummary {
    pub(in crate::vvc) candidate_count: usize,
    pub(in crate::vvc) exact_count: usize,
    pub(in crate::vvc) nonzero_count: usize,
    pub(in crate::vvc) nonzero_exact_count: usize,
    pub(in crate::vvc) uniform_count: usize,
    pub(in crate::vvc) nonzero_uniform_count: usize,
    pub(in crate::vvc) uniform_exact_count: usize,
    pub(in crate::vvc) nonzero_uniform_exact_count: usize,
    pub(in crate::vvc) near_count: usize,
    pub(in crate::vvc) nonzero_near_count: usize,
    pub(in crate::vvc) uniform_near_count: usize,
    pub(in crate::vvc) nonzero_uniform_near_count: usize,
    pub(in crate::vvc) total_sad: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionAggregateCandidate {
    pub(in crate::vvc) origin_x: usize,
    pub(in crate::vvc) origin_y: usize,
    pub(in crate::vvc) width: usize,
    pub(in crate::vvc) height: usize,
    pub(in crate::vvc) ref_origin_x: usize,
    pub(in crate::vvc) ref_origin_y: usize,
    pub(in crate::vvc) mv: VvcLumaMotionVector,
    pub(in crate::vvc) total_sad: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaMotionMap {
    origin_x: usize,
    origin_y: usize,
    block_size: usize,
    blocks_x: usize,
    blocks_y: usize,
    candidates: Vec<VvcLumaMotionCandidate>,
}

pub(in crate::vvc) fn vvc_luma_motion_analysis_for_region(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    region: VvcCtuRegion,
    search_radius: usize,
    near_sad_per_sample: u64,
) -> VvcLumaMotionRegionAnalysis {
    let Some(map) = vvc_luma_motion_map_for_region(current, reference, region, search_radius)
    else {
        return VvcLumaMotionRegionAnalysis::default();
    };
    let mut analysis = map.block_analysis(near_sad_per_sample);
    analysis.aggregate_16x16 = map.aggregate_summary(2, near_sad_per_sample);
    analysis.aggregate_32x32 = map.aggregate_summary(4, near_sad_per_sample);
    analysis.aggregate_64x64 = map.aggregate_summary(8, near_sad_per_sample);
    analysis
}

pub(in crate::vvc) fn vvc_luma_motion_map_for_region(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    region: VvcCtuRegion,
    search_radius: usize,
) -> Option<VvcLumaMotionMap> {
    if current.geometry != reference.geometry || current.format != reference.format {
        return None;
    }

    let x_end = region
        .origin_x
        .saturating_add(region.geometry.width)
        .min(current.geometry.width);
    let y_end = region
        .origin_y
        .saturating_add(region.geometry.height)
        .min(current.geometry.height);
    if x_end < region.origin_x + VVC_LUMA_MOTION_BLOCK
        || y_end < region.origin_y + VVC_LUMA_MOTION_BLOCK
    {
        return None;
    }

    let blocks_x = (x_end - region.origin_x) / VVC_LUMA_MOTION_BLOCK;
    let blocks_y = (y_end - region.origin_y) / VVC_LUMA_MOTION_BLOCK;
    let mut previous_row_mvs = vec![None; blocks_x];
    let mut candidates = Vec::with_capacity(blocks_x * blocks_y);

    for block_y in 0..blocks_y {
        let mut left_mv = None;
        for block_x in 0..blocks_x {
            let block = VvcLumaMotionSearchBlock {
                origin_x: region.origin_x + block_x * VVC_LUMA_MOTION_BLOCK,
                origin_y: region.origin_y + block_y * VVC_LUMA_MOTION_BLOCK,
                width: VVC_LUMA_MOTION_BLOCK,
                height: VVC_LUMA_MOTION_BLOCK,
            };
            let mut predictor_mvs = [VvcLumaMotionVector { x: 0, y: 0 }; 10];
            let mut predictor_count = 0usize;
            if let Some(mv) = left_mv {
                push_motion_predictor(&mut predictor_mvs, &mut predictor_count, mv);
            }
            if let Some(mv) = previous_row_mvs[block_x] {
                push_motion_predictor(&mut predictor_mvs, &mut predictor_count, mv);
            }
            let coarse_step = 8.min(search_radius) as i32;
            if coarse_step > 0 {
                for mv in [
                    VvcLumaMotionVector {
                        x: -coarse_step,
                        y: 0,
                    },
                    VvcLumaMotionVector {
                        x: coarse_step,
                        y: 0,
                    },
                    VvcLumaMotionVector {
                        x: 0,
                        y: -coarse_step,
                    },
                    VvcLumaMotionVector {
                        x: 0,
                        y: coarse_step,
                    },
                    VvcLumaMotionVector {
                        x: -coarse_step,
                        y: -coarse_step,
                    },
                    VvcLumaMotionVector {
                        x: coarse_step,
                        y: -coarse_step,
                    },
                    VvcLumaMotionVector {
                        x: -coarse_step,
                        y: coarse_step,
                    },
                    VvcLumaMotionVector {
                        x: coarse_step,
                        y: coarse_step,
                    },
                ] {
                    push_motion_predictor(&mut predictor_mvs, &mut predictor_count, mv);
                }
            }
            let candidate = vvc_luma_diamond_motion_search(
                current,
                reference,
                block,
                &predictor_mvs[..predictor_count],
                search_radius,
            );
            let candidate = candidate?;
            left_mv = Some(candidate.mv);
            previous_row_mvs[block_x] = Some(candidate.mv);
            candidates.push(candidate);
        }
    }

    Some(VvcLumaMotionMap {
        origin_x: region.origin_x,
        origin_y: region.origin_y,
        block_size: VVC_LUMA_MOTION_BLOCK,
        blocks_x,
        blocks_y,
        candidates,
    })
}

impl VvcLumaMotionMap {
    pub(in crate::vvc) const fn origin_x(&self) -> usize {
        self.origin_x
    }

    pub(in crate::vvc) const fn origin_y(&self) -> usize {
        self.origin_y
    }

    pub(in crate::vvc) const fn block_size(&self) -> usize {
        self.block_size
    }

    pub(in crate::vvc) const fn blocks_x(&self) -> usize {
        self.blocks_x
    }

    pub(in crate::vvc) const fn blocks_y(&self) -> usize {
        self.blocks_y
    }

    pub(in crate::vvc) fn candidate(
        &self,
        block_x: usize,
        block_y: usize,
    ) -> Option<VvcLumaMotionCandidate> {
        if block_x >= self.blocks_x || block_y >= self.blocks_y {
            return None;
        }
        self.candidates
            .get(block_y.checked_mul(self.blocks_x)?.checked_add(block_x)?)
            .copied()
    }

    pub(in crate::vvc) fn uniform_aggregate_candidate(
        &self,
        block_x: usize,
        block_y: usize,
        blocks_per_side: usize,
    ) -> Option<VvcLumaMotionAggregateCandidate> {
        self.uniform_aggregate_rect_candidate(block_x, block_y, blocks_per_side, blocks_per_side)
    }

    pub(in crate::vvc) fn uniform_aggregate_rect_candidate(
        &self,
        block_x: usize,
        block_y: usize,
        blocks_w: usize,
        blocks_h: usize,
    ) -> Option<VvcLumaMotionAggregateCandidate> {
        if blocks_w == 0 || blocks_h == 0 {
            return None;
        }
        let aggregate = self.aggregate_area(block_x, block_y, blocks_w, blocks_h)?;
        if !aggregate.uniform_mv {
            return None;
        }
        let mv = aggregate.mv?;
        let origin_x = self
            .origin_x
            .checked_add(block_x.checked_mul(self.block_size)?)?;
        let origin_y = self
            .origin_y
            .checked_add(block_y.checked_mul(self.block_size)?)?;
        let width = blocks_w.checked_mul(self.block_size)?;
        let height = blocks_h.checked_mul(self.block_size)?;
        let ref_origin_x = offset_origin(origin_x, mv.x)?;
        let ref_origin_y = offset_origin(origin_y, mv.y)?;
        Some(VvcLumaMotionAggregateCandidate {
            origin_x,
            origin_y,
            width,
            height,
            ref_origin_x,
            ref_origin_y,
            mv,
            total_sad: aggregate.total_sad,
        })
    }

    fn block_analysis(&self, near_sad_per_sample: u64) -> VvcLumaMotionRegionAnalysis {
        let summary = self.aggregate_summary(1, near_sad_per_sample);
        VvcLumaMotionRegionAnalysis {
            block_count: summary.candidate_count,
            exact_count: summary.exact_count,
            nonzero_count: summary.nonzero_count,
            nonzero_exact_count: summary.nonzero_exact_count,
            near_count: summary.near_count,
            nonzero_near_count: summary.nonzero_near_count,
            total_sad: summary.total_sad,
            aggregate_16x16: VvcLumaMotionAggregateSummary::default(),
            aggregate_32x32: VvcLumaMotionAggregateSummary::default(),
            aggregate_64x64: VvcLumaMotionAggregateSummary::default(),
        }
    }

    fn aggregate_summary(
        &self,
        blocks_per_side: usize,
        near_sad_per_sample: u64,
    ) -> VvcLumaMotionAggregateSummary {
        if blocks_per_side == 0
            || self.blocks_x < blocks_per_side
            || self.blocks_y < blocks_per_side
        {
            return VvcLumaMotionAggregateSummary::default();
        }

        let mut summary = VvcLumaMotionAggregateSummary::default();
        for block_y in (0..=self.blocks_y - blocks_per_side).step_by(blocks_per_side) {
            for block_x in (0..=self.blocks_x - blocks_per_side).step_by(blocks_per_side) {
                let Some(aggregate) =
                    self.aggregate_area(block_x, block_y, blocks_per_side, blocks_per_side)
                else {
                    continue;
                };
                summary.candidate_count += 1;
                summary.total_sad = summary.total_sad.saturating_add(aggregate.total_sad);
                if aggregate.total_sad == 0 {
                    summary.exact_count += 1;
                    if aggregate.has_nonzero_mv {
                        summary.nonzero_exact_count += 1;
                    }
                }
                if aggregate.has_nonzero_mv {
                    summary.nonzero_count += 1;
                }
                if aggregate.uniform_mv {
                    summary.uniform_count += 1;
                    if aggregate.has_nonzero_mv {
                        summary.nonzero_uniform_count += 1;
                    }
                    if aggregate.total_sad == 0 {
                        summary.uniform_exact_count += 1;
                        if aggregate.has_nonzero_mv {
                            summary.nonzero_uniform_exact_count += 1;
                        }
                    }
                }
                let samples = blocks_per_side
                    .saturating_mul(blocks_per_side)
                    .saturating_mul(self.block_size)
                    .saturating_mul(self.block_size);
                if aggregate.total_sad <= motion_near_threshold(near_sad_per_sample, samples) {
                    summary.near_count += 1;
                    if aggregate.has_nonzero_mv {
                        summary.nonzero_near_count += 1;
                    }
                    if aggregate.uniform_mv {
                        summary.uniform_near_count += 1;
                        if aggregate.has_nonzero_mv {
                            summary.nonzero_uniform_near_count += 1;
                        }
                    }
                }
            }
        }
        summary
    }

    fn aggregate_area(
        &self,
        block_x: usize,
        block_y: usize,
        blocks_w: usize,
        blocks_h: usize,
    ) -> Option<VvcLumaMotionAggregateArea> {
        let mut total_sad = 0u64;
        let mut first_mv = None;
        let mut uniform_mv = true;
        let mut has_nonzero_mv = false;
        for y in block_y..block_y.checked_add(blocks_h)? {
            for x in block_x..block_x.checked_add(blocks_w)? {
                let candidate = self.candidate(x, y)?;
                total_sad = total_sad.saturating_add(candidate.sad);
                if candidate.mv != (VvcLumaMotionVector { x: 0, y: 0 }) {
                    has_nonzero_mv = true;
                }
                match first_mv {
                    Some(mv) if mv != candidate.mv => uniform_mv = false,
                    Some(_) => {}
                    None => first_mv = Some(candidate.mv),
                }
            }
        }
        Some(VvcLumaMotionAggregateArea {
            total_sad,
            uniform_mv,
            has_nonzero_mv,
            mv: first_mv,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcLumaMotionAggregateArea {
    total_sad: u64,
    uniform_mv: bool,
    has_nonzero_mv: bool,
    mv: Option<VvcLumaMotionVector>,
}

fn motion_near_threshold(near_sad_per_sample: u64, samples: usize) -> u64 {
    near_sad_per_sample.saturating_mul(samples as u64)
}

fn push_motion_predictor(
    predictors: &mut [VvcLumaMotionVector],
    count: &mut usize,
    mv: VvcLumaMotionVector,
) {
    if *count >= predictors.len() || predictors[..*count].contains(&mv) {
        return;
    }
    predictors[*count] = mv;
    *count += 1;
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
    if vvc_exact_motion_candidate_allows_early_exit(best) {
        return Some(best);
    }
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
        if let Some(candidate) = vvc_luma_motion_candidate_at_with_sad_limit(
            current,
            reference,
            block,
            ref_origin_x,
            ref_origin_y,
            Some(best.sad),
        ) {
            if candidate.is_better_than(best) {
                best = candidate;
                if vvc_exact_motion_candidate_allows_early_exit(best) {
                    return Some(best);
                }
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
            let Some(candidate) = vvc_luma_motion_candidate_at_with_sad_limit(
                current,
                reference,
                block,
                ref_origin_x,
                ref_origin_y,
                Some(best.sad),
            ) else {
                continue;
            };
            if candidate.is_better_than(best) {
                best = candidate;
                if vvc_exact_motion_candidate_allows_early_exit(best) {
                    return Some(best);
                }
                improved = true;
                break;
            }
        }
        if !improved {
            return Some(best);
        }
    }
}

fn vvc_exact_motion_candidate_allows_early_exit(candidate: VvcLumaMotionCandidate) -> bool {
    candidate.sad == 0
        && motion_vector_tie_cost(candidate.mv) <= VVC_LUMA_EXACT_MOTION_EARLY_EXIT_MAX_TIE_COST
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
    vvc_luma_motion_candidate_at_with_sad_limit(
        current,
        reference,
        block,
        ref_origin_x,
        ref_origin_y,
        None,
    )
}

fn vvc_luma_motion_candidate_at_with_sad_limit(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    block: VvcLumaMotionSearchBlock,
    ref_origin_x: usize,
    ref_origin_y: usize,
    sad_limit: Option<u64>,
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
        sad: vvc_luma_block_sad_limited(
            current,
            reference,
            block,
            ref_origin_x,
            ref_origin_y,
            sad_limit,
        ),
    })
}

fn vvc_luma_block_sad_limited(
    current: &VvcSampledFrame,
    reference: &VvcSampledFrame,
    block: VvcLumaMotionSearchBlock,
    ref_origin_x: usize,
    ref_origin_y: usize,
    sad_limit: Option<u64>,
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
            if sad_limit.is_some_and(|limit| sad > limit) {
                return sad;
            }
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

    #[test]
    fn vvc_luma_motion_analysis_counts_exact_nonzero_motion() {
        let mut reference = motion_test_frame(24, 16);
        for y in 0..8 {
            for x in 0..8 {
                reference.luma[y * 24 + x] = (900 + y * 31 + x * 7) as VvcSample;
            }
        }
        let mut current = motion_test_frame(24, 16);
        for y in 0..8 {
            for x in 0..8 {
                current.luma[y * 24 + x] = reference.luma[y * 24 + x];
                current.luma[y * 24 + 8 + x] = reference.luma[y * 24 + x];
            }
        }

        let analysis = vvc_luma_motion_analysis_for_region(
            &current,
            &reference,
            VvcCtuRegion {
                slice_address: 0,
                origin_x: 0,
                origin_y: 0,
                geometry: VvcVideoGeometry {
                    width: 24,
                    height: 16,
                },
            },
            8,
            0,
        );

        assert_eq!(analysis.block_count, 6);
        assert!(analysis.exact_count >= 1);
        assert!(analysis.nonzero_count >= analysis.nonzero_exact_count);
        assert!(analysis.nonzero_exact_count >= 1);
        assert!(analysis.near_count >= analysis.exact_count);
        assert!(analysis.nonzero_near_count >= analysis.nonzero_exact_count);
    }

    #[test]
    fn vvc_luma_motion_map_summarizes_larger_uniform_motion_candidates() {
        let width = 80;
        let height = 32;
        let mut reference = motion_test_frame(width, height);
        for y in 0..32 {
            for x in 0..32 {
                reference.luma[y * width + x] = (1200 + y * 37 + x * 11) as VvcSample;
            }
        }
        let mut current = motion_test_frame(width, height);
        for y in 0..32 {
            for x in 0..32 {
                current.luma[y * width + 32 + x] = reference.luma[y * width + x];
            }
        }

        let region = VvcCtuRegion {
            slice_address: 0,
            origin_x: 0,
            origin_y: 0,
            geometry: VvcVideoGeometry { width, height },
        };
        let map = vvc_luma_motion_map_for_region(&current, &reference, region, 32)
            .expect("motion map should cover full 8x8 cells");

        assert_eq!(map.origin_x(), 0);
        assert_eq!(map.origin_y(), 0);
        assert_eq!(map.block_size(), 8);
        assert_eq!(map.blocks_x(), 10);
        assert_eq!(map.blocks_y(), 4);
        assert_eq!(
            map.candidate(4, 0).expect("moved block candidate").mv,
            VvcLumaMotionVector { x: -32, y: 0 }
        );
        assert_eq!(map.candidate(map.blocks_x(), 0), None);
        assert_eq!(map.candidate(0, map.blocks_y()), None);

        let analysis = vvc_luma_motion_analysis_for_region(&current, &reference, region, 32, 0);

        assert!(analysis.aggregate_16x16.candidate_count >= 4);
        assert!(analysis.aggregate_16x16.nonzero_uniform_count >= 4);
        assert!(analysis.aggregate_16x16.nonzero_uniform_exact_count >= 4);
        assert!(analysis.aggregate_16x16.nonzero_uniform_near_count >= 4);
        assert!(analysis.aggregate_32x32.candidate_count >= 2);
        assert!(analysis.aggregate_32x32.nonzero_uniform_count >= 1);
        assert!(analysis.aggregate_32x32.nonzero_uniform_exact_count >= 1);
        assert!(analysis.aggregate_32x32.nonzero_uniform_near_count >= 1);

        let aggregate = map
            .uniform_aggregate_candidate(4, 0, 4)
            .expect("shifted 32x32 region has one uniform motion vector");
        assert_eq!(aggregate.origin_x, 32);
        assert_eq!(aggregate.origin_y, 0);
        assert_eq!(aggregate.width, 32);
        assert_eq!(aggregate.height, 32);
        assert_eq!(aggregate.ref_origin_x, 0);
        assert_eq!(aggregate.ref_origin_y, 0);
        assert_eq!(aggregate.mv, VvcLumaMotionVector { x: -32, y: 0 });
        assert_eq!(aggregate.total_sad, 0);
        assert_eq!(map.uniform_aggregate_candidate(4, 0, 0), None);
    }

    #[test]
    fn vvc_luma_motion_map_summarizes_whole_ctu_motion_candidate() {
        let width = 72;
        let height = 64;
        let mut reference = motion_test_frame(width, height);
        for y in 0..64 {
            for x in 8..72 {
                reference.luma[y * width + x] = (1500 + y * 17 + x * 23) as VvcSample;
            }
        }
        let mut current = motion_test_frame(width, height);
        for y in 0..64 {
            for x in 0..64 {
                current.luma[y * width + x] = reference.luma[y * width + 8 + x];
            }
        }

        let region = VvcCtuRegion {
            slice_address: 0,
            origin_x: 0,
            origin_y: 0,
            geometry: VvcVideoGeometry { width: 64, height },
        };
        let analysis = vvc_luma_motion_analysis_for_region(&current, &reference, region, 8, 0);

        assert_eq!(analysis.aggregate_64x64.candidate_count, 1);
        assert!(analysis.aggregate_64x64.nonzero_uniform_count >= 1);
        assert!(analysis.aggregate_64x64.nonzero_uniform_exact_count >= 1);
        assert!(analysis.aggregate_64x64.nonzero_uniform_near_count >= 1);
    }

    #[test]
    fn vvc_luma_motion_map_rejects_mixed_mv_aggregate_candidate() {
        let mut reference = motion_test_frame(24, 16);
        for y in 0..8 {
            for x in 0..8 {
                reference.luma[y * 24 + x] = (900 + y * 31 + x * 7) as VvcSample;
            }
        }
        let mut current = motion_test_frame(24, 16);
        for y in 0..8 {
            for x in 0..8 {
                current.luma[y * 24 + x] = reference.luma[y * 24 + x];
                current.luma[y * 24 + 8 + x] = reference.luma[y * 24 + x];
            }
        }

        let region = VvcCtuRegion {
            slice_address: 0,
            origin_x: 0,
            origin_y: 0,
            geometry: VvcVideoGeometry {
                width: 24,
                height: 16,
            },
        };
        let map = vvc_luma_motion_map_for_region(&current, &reference, region, 8)
            .expect("motion map should cover full 8x8 cells");

        assert_eq!(
            map.candidate(0, 0).expect("static block").mv,
            VvcLumaMotionVector { x: 0, y: 0 }
        );
        assert_eq!(
            map.candidate(1, 0).expect("shifted block").mv,
            VvcLumaMotionVector { x: -8, y: 0 }
        );
        assert_eq!(map.uniform_aggregate_candidate(0, 0, 2), None);
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
