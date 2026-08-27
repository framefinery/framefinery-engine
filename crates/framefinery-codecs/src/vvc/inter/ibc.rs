#[cfg(feature = "vvc-stats")]
use crate::picture::ChromaSampling;
use crate::picture::SampleBitDepth;

use super::{
    vvc_plane_region_is_available, VvcLumaIbcDecision, VvcLumaSccDecision, VvcReconstructionFrame,
    VvcSample, VvcSampledFrame, VVC_CTU_SIZE,
};
#[cfg(feature = "vvc-stats")]
use super::{VvcCtuRegion, VvcSampledColor};

const VVC_IBC_CU_SIZE: usize = 8;
const VVC_IBC_CUS_PER_CTU: usize =
    (VVC_CTU_SIZE / VVC_IBC_CU_SIZE) * (VVC_CTU_SIZE / VVC_IBC_CU_SIZE);
const VVC_IBC_HASH_OFFSET: u32 = 0x811c_9dc5;

pub(in crate::vvc) trait VvcIbcFrameView {
    fn visible_width(&self) -> usize;
    fn visible_height(&self) -> usize;
    fn stride(&self) -> usize;
    fn bit_depth(&self) -> SampleBitDepth;
    fn planes(&self) -> [&[VvcSample]; 3];
    fn block_available(&self, origin_x: usize, origin_y: usize) -> bool;
}

impl VvcIbcFrameView for VvcSampledFrame {
    fn visible_width(&self) -> usize {
        self.geometry.width
    }

    fn visible_height(&self) -> usize {
        self.geometry.height
    }

    fn stride(&self) -> usize {
        self.geometry.width
    }

    fn bit_depth(&self) -> SampleBitDepth {
        self.format.bit_depth
    }

    fn planes(&self) -> [&[VvcSample]; 3] {
        [&self.luma, &self.cb, &self.cr]
    }

    fn block_available(&self, _origin_x: usize, _origin_y: usize) -> bool {
        true
    }
}

impl VvcIbcFrameView for VvcReconstructionFrame {
    fn visible_width(&self) -> usize {
        self.geometry.width
    }

    fn visible_height(&self) -> usize {
        self.geometry.height
    }

    fn stride(&self) -> usize {
        self.luma_width()
    }

    fn bit_depth(&self) -> SampleBitDepth {
        self.format.bit_depth
    }

    fn planes(&self) -> [&[VvcSample]; 3] {
        [&self.luma, &self.cb, &self.cr]
    }

    fn block_available(&self, origin_x: usize, origin_y: usize) -> bool {
        vvc_plane_region_is_available(
            &self.luma_available,
            self.luma_width(),
            origin_x,
            origin_y,
            VVC_IBC_CU_SIZE,
            VVC_IBC_CU_SIZE,
        ) && vvc_plane_region_is_available(
            &self.cb_available,
            self.chroma_width(),
            origin_x,
            origin_y,
            VVC_IBC_CU_SIZE,
            VVC_IBC_CU_SIZE,
        ) && vvc_plane_region_is_available(
            &self.cr_available,
            self.chroma_width(),
            origin_x,
            origin_y,
            VVC_IBC_CU_SIZE,
            VVC_IBC_CU_SIZE,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcIbcCuDecision {
    pub(super) origin_x: usize,
    pub(super) origin_y: usize,
    pub(super) ref_origin_x: usize,
    pub(super) ref_origin_y: usize,
    pub(super) bv_x: i16,
    pub(super) bv_y: i16,
    pub(super) mvd_x: i16,
    pub(super) mvd_y: i16,
    pub(super) pred_mode_ibc_ctx: u8,
}

impl VvcIbcCuDecision {
    /// Convert the search result into the metadata consumed by the shared
    /// luma quantize/reconstruct/CABAC path.
    pub(super) const fn into_luma_scc_decision(self) -> VvcLumaSccDecision {
        VvcLumaSccDecision::IbcExact(VvcLumaIbcDecision {
            mvd_x: self.mvd_x,
            mvd_y: self.mvd_y,
            pred_mode_ibc_ctx: self.pred_mode_ibc_ctx,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcIbcHashEntry {
    hash: u32,
    origin_x: usize,
    origin_y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcIbcBv {
    x: i16,
    y: i16,
}

#[derive(Debug, Clone)]
pub(super) struct VvcIbcHashSearch {
    ctu_origin_x: usize,
    ctu_origin_y: usize,
    entries: Vec<VvcIbcHashEntry>,
    ibc_mode_by_cu: [bool; VVC_IBC_CUS_PER_CTU],
    bv_by_cu: [VvcIbcBv; VVC_IBC_CUS_PER_CTU],
    hmvp: Vec<VvcIbcBv>,
}

impl VvcIbcHashSearch {
    pub(super) fn new() -> Self {
        Self::new_for_ctu(0, 0)
    }

    pub(super) fn new_for_ctu(ctu_origin_x: usize, ctu_origin_y: usize) -> Self {
        Self {
            ctu_origin_x,
            ctu_origin_y,
            entries: Vec::with_capacity(VVC_IBC_CUS_PER_CTU),
            ibc_mode_by_cu: [false; VVC_IBC_CUS_PER_CTU],
            bv_by_cu: [VvcIbcBv { x: 0, y: 0 }; VVC_IBC_CUS_PER_CTU],
            hmvp: Vec::with_capacity(5),
        }
    }

    pub(super) fn decide_8x8<F: VvcIbcFrameView>(
        &self,
        frame: &F,
        origin_x: usize,
        origin_y: usize,
    ) -> Option<VvcIbcCuDecision> {
        if !vvc_ibc_full_8x8_is_visible(frame, origin_x, origin_y) {
            return None;
        }

        let hash = vvc_ibc_hash_8x8(frame, origin_x, origin_y);
        // H.266 8.6.2 allows only already-coded IBC predictors. Keep this
        // first hardware-oriented subset to three local exact-hash candidates
        // so the RTL can resolve a CU as soon as its TU samples arrive instead
        // of synthesizing a 64-way CTU hash search: A1, then B1, then B0.
        let reference = self.local_hash_candidate(origin_x, origin_y, hash)?;
        self.decision_for_reference(origin_x, origin_y, reference)
    }

    pub(super) fn decide_8x8_against_reconstruction<F: VvcIbcFrameView>(
        &self,
        source: &F,
        reference: &VvcReconstructionFrame,
        origin_x: usize,
        origin_y: usize,
    ) -> Option<VvcIbcCuDecision> {
        if !vvc_ibc_full_8x8_is_visible(source, origin_x, origin_y) {
            return None;
        }
        let hash = vvc_ibc_hash_8x8(source, origin_x, origin_y);
        let candidate = self.local_hash_candidate(origin_x, origin_y, hash)?;
        if !reference.block_available(candidate.origin_x, candidate.origin_y) {
            return None;
        }
        self.decision_for_reference(origin_x, origin_y, candidate)
    }

    #[cfg(feature = "vvc-stats")]
    pub(super) fn decide_ctu_hash_8x8<F: VvcIbcFrameView>(
        &self,
        frame: &F,
        origin_x: usize,
        origin_y: usize,
    ) -> Option<VvcIbcCuDecision> {
        if !vvc_ibc_full_8x8_is_visible(frame, origin_x, origin_y) {
            return None;
        }

        let hash = vvc_ibc_hash_8x8(frame, origin_x, origin_y);
        self.ctu_hash_candidates(origin_x, origin_y, hash)
            .filter_map(|reference| self.decision_for_reference(origin_x, origin_y, reference))
            .min_by_key(|decision| vvc_ibc_decision_search_cost(*decision))
    }

    pub(super) fn decide_left_8x8<F: VvcIbcFrameView>(
        &self,
        frame: &F,
        origin_x: usize,
        origin_y: usize,
    ) -> Option<VvcIbcCuDecision> {
        let (local_x, _) = self.local_origin(origin_x, origin_y)?;
        if local_x < VVC_IBC_CU_SIZE || !vvc_ibc_full_8x8_is_visible(frame, origin_x, origin_y) {
            return None;
        }

        let ref_origin_x = origin_x - VVC_IBC_CU_SIZE;
        let ref_origin_y = origin_y;
        if !frame.block_available(ref_origin_x, ref_origin_y) {
            return None;
        }
        let predictor = self.bvp_for(origin_x, origin_y);
        let bv = VvcIbcBv {
            x: -((VVC_IBC_CU_SIZE as i16) << 4),
            y: 0,
        };
        let bvd = VvcIbcBv {
            x: bv.x - predictor.x,
            y: bv.y - predictor.y,
        };

        if (bvd.x & 15) != 0 || (bvd.y & 15) != 0 {
            return None;
        }
        let mvd = VvcIbcBv {
            x: bvd.x >> 4,
            y: bvd.y >> 4,
        };

        if !vvc_ibc_mvd_component_is_supported(mvd.x) || !vvc_ibc_mvd_component_is_supported(mvd.y)
        {
            return None;
        }

        Some(VvcIbcCuDecision {
            origin_x,
            origin_y,
            ref_origin_x,
            ref_origin_y,
            bv_x: bv.x,
            bv_y: bv.y,
            mvd_x: mvd.x,
            mvd_y: mvd.y,
            pred_mode_ibc_ctx: self.pred_mode_ibc_ctx(origin_x, origin_y),
        })
    }

    pub(super) fn record_palette_8x8<F: VvcIbcFrameView>(
        &mut self,
        frame: &F,
        origin_x: usize,
        origin_y: usize,
    ) {
        self.record_mode(origin_x, origin_y, None);
        self.record_hash_if_full_visible(frame, origin_x, origin_y);
    }

    pub(super) fn record_ibc_8x8<F: VvcIbcFrameView>(
        &mut self,
        frame: &F,
        decision: VvcIbcCuDecision,
    ) {
        let bv = VvcIbcBv {
            x: decision.bv_x,
            y: decision.bv_y,
        };
        self.record_mode(decision.origin_x, decision.origin_y, Some(bv));
        self.record_hmvp(bv);
        self.record_hash_if_full_visible(frame, decision.origin_x, decision.origin_y);
    }

    pub(super) fn pred_mode_ibc_ctx(&self, origin_x: usize, origin_y: usize) -> u8 {
        let mut ctx = 0;
        if origin_x >= VVC_IBC_CU_SIZE {
            ctx += u8::from(self.ibc_mode_at(origin_x - VVC_IBC_CU_SIZE, origin_y));
        }
        if origin_y >= VVC_IBC_CU_SIZE {
            ctx += u8::from(self.ibc_mode_at(origin_x, origin_y - VVC_IBC_CU_SIZE));
        }
        ctx
    }

    fn bvp_for(&self, origin_x: usize, origin_y: usize) -> VvcIbcBv {
        let Some((local_x, local_y)) = self.local_origin(origin_x, origin_y) else {
            return VvcIbcBv { x: 0, y: 0 };
        };
        // H.266 8.6.2.2 constructs the IBC BVP list as A1, B1, HMVP, then zero.
        // The current SPS sets MaxNumIbcMergeCand to 1, so only the first
        // available candidate is used and mvp_l0_flag is not present.
        if local_x >= VVC_IBC_CU_SIZE {
            if let Some(bv) = self.ibc_bv_at_local(local_x - VVC_IBC_CU_SIZE, local_y) {
                return bv;
            }
        }
        if local_y >= VVC_IBC_CU_SIZE {
            if let Some(bv) = self.ibc_bv_at_local(local_x, local_y - VVC_IBC_CU_SIZE) {
                return bv;
            }
        }
        self.hmvp.last().copied().unwrap_or(VvcIbcBv { x: 0, y: 0 })
    }

    fn record_mode(&mut self, origin_x: usize, origin_y: usize, bv: Option<VvcIbcBv>) {
        let Some((local_x, local_y)) = self.local_origin(origin_x, origin_y) else {
            return;
        };
        let Some(index) = vvc_ibc_cu_index(local_x, local_y) else {
            return;
        };
        self.ibc_mode_by_cu[index] = bv.is_some();
        self.bv_by_cu[index] = bv.unwrap_or(VvcIbcBv { x: 0, y: 0 });
    }

    fn record_hash_if_full_visible<F: VvcIbcFrameView>(
        &mut self,
        frame: &F,
        origin_x: usize,
        origin_y: usize,
    ) {
        if vvc_ibc_full_8x8_is_visible(frame, origin_x, origin_y)
            && frame.block_available(origin_x, origin_y)
        {
            self.entries.push(VvcIbcHashEntry {
                hash: vvc_ibc_hash_8x8(frame, origin_x, origin_y),
                origin_x,
                origin_y,
            });
        }
    }

    fn local_hash_candidate(
        &self,
        origin_x: usize,
        origin_y: usize,
        hash: u32,
    ) -> Option<VvcIbcHashEntry> {
        let (local_x, local_y) = self.local_origin(origin_x, origin_y)?;
        if local_x >= VVC_IBC_CU_SIZE {
            if let Some(entry) = self.hash_entry_at(origin_x - VVC_IBC_CU_SIZE, origin_y) {
                if entry.hash == hash {
                    return Some(entry);
                }
            }
        }
        if local_y >= VVC_IBC_CU_SIZE {
            if let Some(entry) = self.hash_entry_at(origin_x, origin_y - VVC_IBC_CU_SIZE) {
                if entry.hash == hash {
                    return Some(entry);
                }
            }
        }
        if local_x >= VVC_IBC_CU_SIZE && local_y >= VVC_IBC_CU_SIZE {
            if let Some(entry) =
                self.hash_entry_at(origin_x - VVC_IBC_CU_SIZE, origin_y - VVC_IBC_CU_SIZE)
            {
                if entry.hash == hash {
                    return Some(entry);
                }
            }
        }
        None
    }

    fn record_hmvp(&mut self, bv: VvcIbcBv) {
        if let Some(pos) = self.hmvp.iter().position(|entry| *entry == bv) {
            self.hmvp.remove(pos);
        } else if self.hmvp.len() == 5 {
            self.hmvp.remove(0);
        }
        self.hmvp.push(bv);
    }

    fn ibc_mode_at(&self, origin_x: usize, origin_y: usize) -> bool {
        let Some((local_x, local_y)) = self.local_origin(origin_x, origin_y) else {
            return false;
        };
        self.ibc_mode_at_local(local_x, local_y)
    }

    fn ibc_mode_at_local(&self, local_x: usize, local_y: usize) -> bool {
        vvc_ibc_cu_index(local_x, local_y)
            .map(|index| self.ibc_mode_by_cu[index])
            .unwrap_or(false)
    }

    fn ibc_bv_at_local(&self, local_x: usize, local_y: usize) -> Option<VvcIbcBv> {
        let index = vvc_ibc_cu_index(local_x, local_y)?;
        self.ibc_mode_by_cu[index].then_some(self.bv_by_cu[index])
    }

    fn hash_entry_at(&self, origin_x: usize, origin_y: usize) -> Option<VvcIbcHashEntry> {
        self.entries
            .iter()
            .find(|entry| entry.origin_x == origin_x && entry.origin_y == origin_y)
            .copied()
    }

    #[cfg(feature = "vvc-stats")]
    fn ctu_hash_candidates(
        &self,
        origin_x: usize,
        origin_y: usize,
        hash: u32,
    ) -> impl Iterator<Item = VvcIbcHashEntry> + '_ {
        self.entries.iter().copied().filter(move |entry| {
            entry.hash == hash
                && (entry.origin_y < origin_y
                    || (entry.origin_y == origin_y && entry.origin_x < origin_x))
        })
    }

    fn decision_for_reference(
        &self,
        origin_x: usize,
        origin_y: usize,
        reference: VvcIbcHashEntry,
    ) -> Option<VvcIbcCuDecision> {
        let predictor = self.bvp_for(origin_x, origin_y);
        let bv = VvcIbcBv {
            x: ((reference.origin_x as i16 - origin_x as i16) << 4),
            y: ((reference.origin_y as i16 - origin_y as i16) << 4),
        };
        let bvd = VvcIbcBv {
            x: bv.x - predictor.x,
            y: bv.y - predictor.y,
        };

        if (bvd.x & 15) != 0 || (bvd.y & 15) != 0 {
            return None;
        }
        let mvd = VvcIbcBv {
            x: bvd.x >> 4,
            y: bvd.y >> 4,
        };

        // H.266 7.3.11.8 codes lMvd before the IBC AMVR scaling specified by
        // H.266 Table 16. Our 8x8 hash search uses integer-sample BVs, so the
        // coded MVD is the 1/16-sample BVD divided by 16. The current CTU-local
        // table is far inside the [-2^17, 2^17-1] range, but keep the guard
        // here so a later picture-wide IBC virtual buffer has a clear failure
        // point.
        if !vvc_ibc_mvd_component_is_supported(mvd.x) || !vvc_ibc_mvd_component_is_supported(mvd.y)
        {
            return None;
        }

        Some(VvcIbcCuDecision {
            origin_x,
            origin_y,
            ref_origin_x: reference.origin_x,
            ref_origin_y: reference.origin_y,
            bv_x: bv.x,
            bv_y: bv.y,
            mvd_x: mvd.x,
            mvd_y: mvd.y,
            pred_mode_ibc_ctx: self.pred_mode_ibc_ctx(origin_x, origin_y),
        })
    }

    fn local_origin(&self, origin_x: usize, origin_y: usize) -> Option<(usize, usize)> {
        let local_x = origin_x.checked_sub(self.ctu_origin_x)?;
        let local_y = origin_y.checked_sub(self.ctu_origin_y)?;
        (local_x < VVC_CTU_SIZE && local_y < VVC_CTU_SIZE).then_some((local_x, local_y))
    }
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct VvcSccCtuAnalysis {
    pub(super) block_count: usize,
    pub(super) palette_solid_8x8_count: usize,
    pub(super) palette_no_escape_8x8_count: usize,
    pub(super) palette_escape_8x8_count: usize,
    pub(super) ibc_exact_8x8_count: usize,
    pub(super) ibc_ctu_exact_8x8_count: usize,
    pub(super) ibc_ctu_extra_exact_8x8_count: usize,
    pub(super) ibc_left_residual_8x8_count: usize,
}

#[cfg(feature = "vvc-stats")]
pub(super) fn vvc_scc_analysis_for_region(
    frame: &VvcSampledFrame,
    region: VvcCtuRegion,
) -> VvcSccCtuAnalysis {
    if frame.format.chroma_sampling != ChromaSampling::Cs444 {
        return VvcSccCtuAnalysis::default();
    }

    let mut analysis = VvcSccCtuAnalysis::default();
    let mut ibc_search = VvcIbcHashSearch::new_for_ctu(region.origin_x, region.origin_y);
    let x_end = region
        .origin_x
        .saturating_add(region.geometry.width)
        .min(frame.geometry.width);
    let y_end = region
        .origin_y
        .saturating_add(region.geometry.height)
        .min(frame.geometry.height);
    for origin_y in (region.origin_y..y_end).step_by(VVC_IBC_CU_SIZE) {
        if origin_y + VVC_IBC_CU_SIZE > y_end {
            continue;
        }
        for origin_x in (region.origin_x..x_end).step_by(VVC_IBC_CU_SIZE) {
            if origin_x + VVC_IBC_CU_SIZE > x_end {
                continue;
            }
            analysis.block_count += 1;
            let unique_colors = vvc_unique_color_count_8x8(frame, origin_x, origin_y);
            if unique_colors == 1 {
                analysis.palette_solid_8x8_count += 1;
            }
            if unique_colors <= 31 {
                analysis.palette_no_escape_8x8_count += 1;
            } else {
                analysis.palette_escape_8x8_count += 1;
            }
            if let Some(decision) = ibc_search.decide_8x8(frame, origin_x, origin_y) {
                analysis.ibc_exact_8x8_count += 1;
                analysis.ibc_ctu_exact_8x8_count += 1;
                ibc_search.record_ibc_8x8(frame, decision);
            } else {
                if ibc_search
                    .decide_ctu_hash_8x8(frame, origin_x, origin_y)
                    .is_some()
                {
                    analysis.ibc_ctu_exact_8x8_count += 1;
                    analysis.ibc_ctu_extra_exact_8x8_count += 1;
                }
                if ibc_search
                    .decide_left_8x8(frame, origin_x, origin_y)
                    .is_some()
                {
                    analysis.ibc_left_residual_8x8_count += 1;
                }
                ibc_search.record_palette_8x8(frame, origin_x, origin_y);
            }
        }
    }
    analysis
}

#[cfg(feature = "vvc-stats")]
fn vvc_unique_color_count_8x8<F: VvcIbcFrameView>(
    frame: &F,
    origin_x: usize,
    origin_y: usize,
) -> usize {
    let mut colors = Vec::<VvcSampledColor>::with_capacity(32);
    let [luma, cb, cr] = frame.planes();
    for y_off in 0..VVC_IBC_CU_SIZE {
        for x_off in 0..VVC_IBC_CU_SIZE {
            let index = (origin_y + y_off) * frame.stride() + origin_x + x_off;
            let color = VvcSampledColor {
                y: luma[index],
                u: cb[index],
                v: cr[index],
            };
            if colors.iter().all(|entry| *entry != color) {
                colors.push(color);
                if colors.len() > 31 {
                    return colors.len();
                }
            }
        }
    }
    colors.len()
}

fn vvc_ibc_full_8x8_is_visible<F: VvcIbcFrameView>(
    frame: &F,
    origin_x: usize,
    origin_y: usize,
) -> bool {
    origin_x + VVC_IBC_CU_SIZE <= frame.visible_width()
        && origin_y + VVC_IBC_CU_SIZE <= frame.visible_height()
}

fn vvc_ibc_cu_index(origin_x: usize, origin_y: usize) -> Option<usize> {
    let col = origin_x / VVC_IBC_CU_SIZE;
    let row = origin_y / VVC_IBC_CU_SIZE;
    if col < VVC_CTU_SIZE / VVC_IBC_CU_SIZE && row < VVC_CTU_SIZE / VVC_IBC_CU_SIZE {
        Some(row * (VVC_CTU_SIZE / VVC_IBC_CU_SIZE) + col)
    } else {
        None
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_ibc_decision_search_cost(decision: VvcIbcCuDecision) -> (u32, u32, usize, usize) {
    let mvd_cost = u32::from(decision.mvd_x.unsigned_abs())
        .saturating_add(u32::from(decision.mvd_y.unsigned_abs()));
    let bv_cost = u32::from(decision.bv_x.unsigned_abs())
        .saturating_add(u32::from(decision.bv_y.unsigned_abs()));
    (
        mvd_cost,
        bv_cost,
        decision.ref_origin_y,
        decision.ref_origin_x,
    )
}

fn vvc_ibc_hash_8x8<F: VvcIbcFrameView>(frame: &F, origin_x: usize, origin_y: usize) -> u32 {
    let mut hash = VVC_IBC_HASH_OFFSET;
    // Mirror ff_vvc_ibc_hash_matcher.sv and the top-level TU stream contract:
    // one 8x8 luma block, then the colocated 8x8 Cb block, then Cr.
    for plane in frame.planes() {
        for y_off in 0..VVC_IBC_CU_SIZE {
            for x_off in 0..VVC_IBC_CU_SIZE {
                let sample_x = origin_x + x_off;
                let sample_y = origin_y + y_off;
                let index = sample_y * frame.stride() + sample_x;
                hash = vvc_ibc_hash_sample(hash, plane[index], frame.bit_depth());
            }
        }
    }
    hash
}

fn vvc_ibc_hash_sample(hash: u32, value: VvcSample, bit_depth: SampleBitDepth) -> u32 {
    let hash = vvc_ibc_hash_byte(hash, value as u8);
    if bit_depth.bits() > 8 {
        vvc_ibc_hash_byte(hash, (value >> 8) as u8)
    } else {
        hash
    }
}

fn vvc_ibc_hash_byte(hash: u32, value: u8) -> u32 {
    let mixed = hash ^ u32::from(value);
    let mixed = mixed ^ mixed.wrapping_shl(13);
    let mixed = mixed ^ mixed.wrapping_shr(17);
    mixed ^ mixed.wrapping_shl(5)
}

fn vvc_ibc_mvd_component_is_supported(value: i16) -> bool {
    (-131_072..=131_071).contains(&i32::from(value))
}
