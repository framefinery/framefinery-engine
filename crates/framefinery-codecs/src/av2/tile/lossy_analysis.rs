fn prune_regular_dct_ac_levels(
    qcoeff: &mut [i32; TX4X4_SAMPLES],
    qindex: u16,
    bit_depth: SampleBitDepth,
    chroma_format: Av2ChromaFormat,
    source_variance: usize,
) {
    let threshold =
        regular_dct_ac_prune_threshold(qindex, bit_depth, chroma_format, source_variance);
    if threshold == 0 {
        return;
    }
    for coeff in qcoeff.iter_mut().skip(1) {
        if coeff.abs() <= threshold {
            *coeff = 0;
        }
    }
}

fn prune_regular_dct_trailing_unit_acs(
    qcoeff: &mut [i32; TX4X4_SAMPLES],
    max_pruned: usize,
) -> usize {
    let mut pruned = 0usize;
    for scan_index in (1..TX4X4_SAMPLES).rev() {
        let pos = TX4X4_SCAN[scan_index];
        match qcoeff[pos].abs() {
            0 => continue,
            1 => {
                qcoeff[pos] = 0;
                pruned += 1;
                if pruned == max_pruned {
                    return pruned;
                }
            }
            _ => return pruned,
        }
    }
    pruned
}

fn regular_dct_ac_prune_threshold(
    qindex: u16,
    bit_depth: SampleBitDepth,
    chroma_format: Av2ChromaFormat,
    source_variance: usize,
) -> i32 {
    if bit_depth.bits() <= 8 {
        return if chroma_format == Av2ChromaFormat::Yuv444
            && qindex >= 72
            && source_variance <= 16384
        {
            1
        } else {
            0
        };
    }
    match qindex {
        0..=71 => 0,
        72..=111 => 3,
        112..=159 => 3,
        _ => 4,
    }
}

#[cfg(feature = "av2-lossy-stats")]
impl Drop for Av2LossySubsampledTileState<'_> {
    fn drop(&mut self) {
        if let Some(stats) = &self.stats {
            stats.borrow().print(self.region, self.chroma_format, self.bit_depth, self.qp);
        }
    }
}

#[cfg(feature = "av2-lossy-stats")]
fn av2_lossy_stats_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("FRAMEFINERY_AV2_LOSSY_STATS").is_some_and(|value| value != "0")
    })
}

fn txb_dc_recon_distortion_with_source_variance(
    source: &[Av2Sample; TX4X4_SAMPLES],
    predictor: &[Av2Sample; TX4X4_SAMPLES],
    delta: i16,
    bit_depth: SampleBitDepth,
    source_variance: usize,
) -> (usize, usize) {
    let mut sse = 0usize;
    let mut recon_samples = [0i32; TX4X4_SAMPLES];
    let max_sample = i32::from(bit_depth.max_sample());
    for index in 0..TX4X4_SAMPLES {
        let recon = (i32::from(predictor[index]) + i32::from(delta)).clamp(0, max_sample);
        recon_samples[index] = recon;
        let diff = i32::from(source[index]) - recon;
        sse += (diff * diff) as usize;
    }
    (sse, txb_recon_variance_loss(source_variance, &recon_samples))
}

fn txb_source_variance(source: &[Av2Sample; TX4X4_SAMPLES]) -> usize {
    let mut samples = [0i32; TX4X4_SAMPLES];
    for (sample, out) in source.iter().zip(samples.iter_mut()) {
        *out = i32::from(*sample);
    }
    txb_variance_measure(&samples)
}

fn txb_recon_variance_loss(source_variance: usize, recon: &[i32; TX4X4_SAMPLES]) -> usize {
    source_variance.saturating_sub(txb_variance_measure(recon))
}

fn txb_recon_sse_and_variance_loss(
    source: &[Av2Sample; TX4X4_SAMPLES],
    predictor: &[Av2Sample; TX4X4_SAMPLES],
    residual: &[i32; TX4X4_SAMPLES],
    max_sample: i32,
    source_variance: usize,
) -> (usize, usize) {
    let mut sse = 0usize;
    let mut sum = 0i64;
    let mut sum_sq = 0i64;
    for index in 0..TX4X4_SAMPLES {
        let recon = (i32::from(predictor[index]) + residual[index]).clamp(0, max_sample);
        let recon_i64 = i64::from(recon);
        sum += recon_i64;
        sum_sq += recon_i64 * recon_i64;
        let diff = i32::from(source[index]) - recon;
        sse += (diff * diff) as usize;
    }
    let sample_count = TX4X4_SAMPLES as i64;
    let mean_sq = (sum * sum + sample_count / 2) / sample_count;
    let variance = sum_sq.saturating_sub(mean_sq) as usize;
    (sse, source_variance.saturating_sub(variance))
}

fn dpcm_recon_samples_and_sse(
    analysis: &Av2LossyTxbAnalysis,
    residual: &[i32; TX4X4_SAMPLES],
    horz: bool,
    max_sample: i32,
) -> ([i32; TX4X4_SAMPLES], usize) {
    let mut recon_samples = [0i32; TX4X4_SAMPLES];
    let mut sse = 0usize;
    for local_y in 0..TX4X4_SIZE {
        for local_x in 0..TX4X4_SIZE {
            let index = local_y * TX4X4_SIZE + local_x;
            let predictor = if horz {
                if local_x == 0 {
                    i32::from(analysis.predictor[index])
                } else {
                    recon_samples[index - 1]
                }
            } else if local_y == 0 {
                i32::from(analysis.predictor[index])
            } else {
                recon_samples[index - TX4X4_SIZE]
            };
            let recon = (predictor + residual[index]).clamp(0, max_sample);
            recon_samples[index] = recon;
            let diff = i32::from(analysis.source[index]) - recon;
            sse += (diff * diff) as usize;
        }
    }
    (recon_samples, sse)
}

fn txb_variance_measure(samples: &[i32; TX4X4_SAMPLES]) -> usize {
    let mut sum = 0i64;
    let mut sum_sq = 0i64;
    for &sample in samples {
        let sample = i64::from(sample);
        sum += sample;
        sum_sq += sample * sample;
    }
    let n = TX4X4_SAMPLES as i64;
    let mean_sq = (sum * sum + n / 2) / n;
    sum_sq.saturating_sub(mean_sq) as usize
}

fn lossy_dc_delta_quant_step(quant_step: i32) -> i32 {
    (quant_step / 16).max(1)
}

fn lossy_transform_coeff_step(quant_step: i32) -> i32 {
    (quant_step.max(1) * 2).max(8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2LossyPlane {
    Y,
    U,
    V,
}

impl Av2LossyPlane {
    fn planar(self) -> Av2PlanarPlane {
        match self {
            Self::Y => Av2PlanarPlane::Y,
            Self::U => Av2PlanarPlane::U,
            Self::V => Av2PlanarPlane::V,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyTx8x8Analysis {
    leaf_x0: usize,
    leaf_y0: usize,
    visible_width: usize,
    visible_height: usize,
    predictor: [Av2Sample; TX8X8_SAMPLES],
    residual: [i32; TX8X8_SAMPLES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyTx8x8QuantizedResidualCandidate {
    coefficients: [i32; TX8X8_SAMPLES],
    residual: [i32; TX8X8_SAMPLES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyTx4x8QuantizedResidualCandidate {
    coefficients: [i32; TX4X8_SAMPLES],
    residual: [i32; TX4X8_SAMPLES],
}

#[derive(Debug, Clone, Copy)]
struct Av2Chroma444IntraPredictorPlan {
    leaf_x0: usize,
    leaf_y0: usize,
    visible_width: usize,
    visible_height: usize,
    have_left: bool,
    have_top: bool,
    dc_pred: Av2Sample,
    above_left: Av2Sample,
    mode: Av2ChromaIntraMode,
    bit_depth: SampleBitDepth,
}

impl Av2Chroma444IntraPredictorPlan {
    fn sample(
        self,
        lossy: &Av2LossySubsampledTileState<'_>,
        plane: Av2LossyPlane,
        local_x: usize,
        local_y: usize,
    ) -> Av2Sample {
        let left = if self.have_left {
            lossy.recon_sample(plane, self.leaf_x0 - 1, self.leaf_y0 + local_y)
        } else if self.have_top {
            lossy.recon_sample(plane, self.leaf_x0, self.leaf_y0 - 1)
        } else {
            av2_lossless_h_pred_left_edge(self.bit_depth)
        };
        let above = if self.have_top {
            lossy.recon_sample(plane, self.leaf_x0 + local_x, self.leaf_y0 - 1)
        } else if self.have_left {
            lossy.recon_sample(plane, self.leaf_x0 - 1, self.leaf_y0)
        } else {
            av2_lossless_v_pred_above_edge(self.bit_depth)
        };
        match self.mode {
            Av2ChromaIntraMode::Dc => self.dc_pred,
            Av2ChromaIntraMode::Horizontal => left,
            Av2ChromaIntraMode::Vertical => above,
            Av2ChromaIntraMode::Paeth => paeth_predictor(left, above, self.above_left),
            _ => av2_lossless_dc_predictor(self.bit_depth),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossySubsampledModeDecision {
    luma_intra_mode: Av2LumaIntraMode,
    luma_bdpcm_horz: Option<bool>,
    chroma_use_bdpcm: bool,
    chroma_intra_mode: Av2ChromaIntraMode,
    use_fsc: bool,
}

impl Default for Av2LossySubsampledModeDecision {
    fn default() -> Self {
        Self {
            luma_intra_mode: Av2LumaIntraMode::Dc,
            luma_bdpcm_horz: None,
            chroma_use_bdpcm: false,
            chroma_intra_mode: Av2ChromaIntraMode::Horizontal,
            use_fsc: false,
        }
    }
}

impl Av2LossySubsampledModeDecision {
    fn coded_luma_mode(self) -> Av2LumaIntraMode {
        match self.luma_bdpcm_horz {
            Some(true) => Av2LumaIntraMode::Horizontal,
            Some(false) => Av2LumaIntraMode::Vertical,
            None => self.luma_intra_mode,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Av2LossyIntraTxbScores {
    dc: usize,
    horizontal: usize,
    vertical: usize,
    paeth: usize,
    smooth: usize,
    smooth_vertical: usize,
    smooth_horizontal: usize,
}

impl Av2LossyIntraTxbScores {
    fn add_assign(&mut self, other: Self) {
        self.dc += other.dc;
        self.horizontal += other.horizontal;
        self.vertical += other.vertical;
        self.paeth += other.paeth;
        self.smooth += other.smooth;
        self.smooth_vertical += other.smooth_vertical;
        self.smooth_horizontal += other.smooth_horizontal;
    }

    fn scaled_to_txb_count(self, total_txbs: usize, sampled_txbs: usize) -> Self {
        if sampled_txbs == 0 || sampled_txbs == total_txbs {
            return self;
        }
        Self {
            dc: self.dc.saturating_mul(total_txbs) / sampled_txbs,
            horizontal: self.horizontal.saturating_mul(total_txbs) / sampled_txbs,
            vertical: self.vertical.saturating_mul(total_txbs) / sampled_txbs,
            paeth: self.paeth.saturating_mul(total_txbs) / sampled_txbs,
            smooth: self.smooth.saturating_mul(total_txbs) / sampled_txbs,
            smooth_vertical: self.smooth_vertical.saturating_mul(total_txbs) / sampled_txbs,
            smooth_horizontal: self.smooth_horizontal.saturating_mul(total_txbs) / sampled_txbs,
        }
    }
}

fn lossy_mode_search_samples_txb(row: usize, col: usize, width: usize, height: usize) -> bool {
    const FULL_SEARCH_TXB_LIMIT: usize = 64;
    if width * height <= FULL_SEARCH_TXB_LIMIT {
        return true;
    }
    (row % 2 == 0 && col % 2 == 0) || row + 1 == height || col + 1 == width
}

fn lossy_scale_sampled_score(score: usize, total_txbs: usize, sampled_txbs: usize) -> usize {
    if sampled_txbs == 0 || sampled_txbs == total_txbs {
        return score;
    }
    score.saturating_mul(total_txbs) / sampled_txbs
}

fn lossy_regular_q_dc_refinement_allowed(
    selected_score: usize,
    dc_score: usize,
    total_txbs: usize,
) -> bool {
    let txb_count = total_txbs.max(1);
    dc_score <= selected_score.saturating_add((selected_score / 16).max(txb_count * 64))
}

fn lossy_regular_q_refinement_selects_dc(
    selected_score: usize,
    dc_score: usize,
    total_txbs: usize,
    quant_step: i32,
) -> bool {
    let txb_count = total_txbs.max(1);
    let quant_margin = usize::try_from(quant_step.max(1)).unwrap_or(1);
    let margin = (selected_score / 256).max(txb_count * quant_margin / 4);
    dc_score.saturating_add(margin) <= selected_score
}

fn lossy_luma_refinement_syntax_penalty(
    mode: Av2LumaIntraMode,
    syntax: Av2LumaModeSyntax,
) -> usize {
    match mode {
        Av2LumaIntraMode::Smooth
        | Av2LumaIntraMode::SmoothVertical
        | Av2LumaIntraMode::SmoothHorizontal => 192,
        _ => lossy_luma_mode_syntax_penalty(mode, syntax),
    }
}

fn lossy_luma_smooth_search_allowed(scores: Av2LossyIntraTxbScores, total_txbs: usize) -> bool {
    let txb_count = total_txbs.max(1);
    let best_axis = scores.horizontal.min(scores.vertical);
    let worst_axis = scores.horizontal.max(scores.vertical);
    let best_simple = scores.dc.min(best_axis).min(scores.paeth);
    let per_txb_residual = best_simple / txb_count;
    if per_txb_residual < 1024 {
        return false;
    }

    let axis_gap = worst_axis.saturating_sub(best_axis);
    axis_gap <= (best_axis / 3).max(txb_count * 128)
}

fn lossy_luma_directional_search_allowed(
    scores: Av2LossyIntraTxbScores,
    total_txbs: usize,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
) -> bool {
    // The current 4x4 directional search helps YUV screen-content edges but
    // regresses the RGB screen-capture row. Keep 8-bit 4:4:4 on the cheaper
    // DC/H/V/Paeth/smooth set until palette or larger-transform decisions can
    // model RGB screen content directly.
    if chroma_format == Av2ChromaFormat::Yuv444 && bit_depth.bits() <= 8 {
        return false;
    }

    let txb_count = total_txbs.max(1);
    let best_axis = scores.horizontal.min(scores.vertical);
    let worst_axis = scores.horizontal.max(scores.vertical);
    let best_simple = scores.dc.min(best_axis).min(scores.paeth);
    let per_txb_residual = best_simple / txb_count;
    let bit_depth_scale = 1usize << usize::from(bit_depth.bits() - 8);
    if per_txb_residual < 1024 * bit_depth_scale {
        return false;
    }

    let axis_gap = worst_axis.saturating_sub(best_axis);
    axis_gap <= best_axis.max(txb_count * 256 * bit_depth_scale)
}

fn lossy_chroma_smooth_search_allowed(
    _scores: Av2LossyIntraTxbScores,
    _total_txbs: usize,
) -> bool {
    // Chroma smooth remains disabled until a content set shows a measured win.
    false
}

fn lossy_chroma_paeth_search_allowed(scores: Av2LossyIntraTxbScores, total_txbs: usize) -> bool {
    let txb_count = total_txbs.max(1);
    let best_axis = scores.horizontal.min(scores.vertical);
    let worst_axis = scores.horizontal.max(scores.vertical);
    let per_txb_residual = scores.dc.min(best_axis) / txb_count;
    if per_txb_residual < 768 {
        return false;
    }

    let axis_gap = worst_axis.saturating_sub(best_axis);
    let max_axis_gap = (best_axis / 2).max(txb_count * 128);
    axis_gap <= max_axis_gap
}

fn lossy_fsc_search_allowed(total_txbs: usize) -> bool {
    let _ = total_txbs;
    false
}

fn lossy_luma_mode_syntax_penalty(
    mode: Av2LumaIntraMode,
    syntax: Av2LumaModeSyntax,
) -> usize {
    match mode {
        Av2LumaIntraMode::Dc => 0,
        Av2LumaIntraMode::Horizontal | Av2LumaIntraMode::Vertical => {
            let index = usize::from(syntax.index_for(mode));
            32 + index.saturating_sub(6) * 128
        }
        Av2LumaIntraMode::Paeth => 128,
        mode if lossy_luma_idif_angle(mode).is_some() => {
            let index = usize::from(syntax.index_for(mode));
            192 + index.saturating_sub(7) * 16
        }
        _ => unreachable!("AV2 lossy luma syntax penalty handles scored modes"),
    }
}

fn lossy_chroma_mode_syntax_penalty(
    luma_mode: Av2LumaIntraMode,
    chroma_mode: Av2ChromaIntraMode,
) -> usize {
    let index = chroma_uv_mode_index(luma_mode, chroma_mode);
    index.min(7) * 32 + usize::from(index >= 7) * 64
}

fn lossy_luma_idif_angle(mode: Av2LumaIntraMode) -> Option<i16> {
    let (base, delta) = mode.directional()?;
    let angle = base.angle(delta);
    (angle != 90 && angle != 180).then_some(angle)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2LossyTxbChoice {
    DcDelta(i16),
    QuantizedResidual(Av2LossyQuantizedResidualCandidate),
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyTxbAnalysis {
    source: [Av2Sample; TX4X4_SAMPLES],
    predictor: [Av2Sample; TX4X4_SAMPLES],
    residual: [i32; TX4X4_SAMPLES],
    delta: i16,
    dc_sse: usize,
    dc_variance_loss: usize,
    source_variance: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyQuantizedResidualCandidate {
    kind: Av2LossyResidualCandidateKind,
    residual: [i32; TX4X4_SAMPLES],
    coefficients: [i32; TX4X4_SAMPLES],
    sse: usize,
    variance_loss: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LossyRegularDctCandidates {
    transform: Av2LossyQuantizedResidualCandidate,
    tail_pruned: Option<Av2LossyQuantizedResidualCandidate>,
    double_tail_pruned: Option<Av2LossyQuantizedResidualCandidate>,
    dc_only: Av2LossyQuantizedResidualCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2LossyResidualCandidateKind {
    Spatial,
    RefinedSpatial,
    Transform,
    RegularDct,
    RegularDctTailPruned,
    RegularDctDoubleTailPruned,
    RegularDctDcOnly,
}

include!("lossy_stats.rs");
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2LosslessSubsampledModeDecision {
    luma_intra_mode: Av2LumaIntraMode,
    luma_bdpcm_horz: Option<bool>,
    chroma_use_bdpcm: bool,
    chroma_intra_mode: Av2ChromaIntraMode,
    use_luma_palette: bool,
    use_fsc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2LosslessSubsampledModeSearch {
    Exhaustive,
    FastScreenContent,
}

impl Default for Av2LosslessSubsampledModeDecision {
    fn default() -> Self {
        Self {
            luma_intra_mode: Av2LumaIntraMode::Dc,
            luma_bdpcm_horz: None,
            chroma_use_bdpcm: false,
            chroma_intra_mode: Av2ChromaIntraMode::Horizontal,
            use_luma_palette: false,
            use_fsc: false,
        }
    }
}

impl Av2LosslessSubsampledModeDecision {
    fn coded_luma_mode(self) -> Av2LumaIntraMode {
        match self.luma_bdpcm_horz {
            Some(true) => Av2LumaIntraMode::Horizontal,
            Some(false) => Av2LumaIntraMode::Vertical,
            None => self.luma_intra_mode,
        }
    }
}

fn chroma_mode_for_luma_mode(mode: Av2LumaIntraMode) -> Av2ChromaIntraMode {
    match mode {
        Av2LumaIntraMode::Dc => Av2ChromaIntraMode::Dc,
        Av2LumaIntraMode::Smooth => Av2ChromaIntraMode::Smooth,
        Av2LumaIntraMode::SmoothVertical => Av2ChromaIntraMode::SmoothVertical,
        Av2LumaIntraMode::SmoothHorizontal => Av2ChromaIntraMode::SmoothHorizontal,
        Av2LumaIntraMode::Paeth => Av2ChromaIntraMode::Paeth,
        Av2LumaIntraMode::Directional45 => Av2ChromaIntraMode::Directional45,
        Av2LumaIntraMode::Directional67 => Av2ChromaIntraMode::Directional67,
        Av2LumaIntraMode::Vertical => Av2ChromaIntraMode::Vertical,
        Av2LumaIntraMode::Directional113 => Av2ChromaIntraMode::Directional113,
        Av2LumaIntraMode::Directional135 => Av2ChromaIntraMode::Directional135,
        Av2LumaIntraMode::Directional157 => Av2ChromaIntraMode::Directional157,
        Av2LumaIntraMode::Horizontal => Av2ChromaIntraMode::Horizontal,
        Av2LumaIntraMode::Directional203 => Av2ChromaIntraMode::Directional203,
        Av2LumaIntraMode::DirectionalDelta { base, .. } => base.chroma_mode(),
    }
}

