struct Av2LossySubsampledTileState<'a> {
    #[cfg(feature = "av2-lossy-stats")]
    region: Av2TileRegion,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &'a [u8],
    recon: &'a mut [u8],
    layout: Av2PlanarTileLayout,
    #[cfg(feature = "av2-lossy-stats")]
    qp: u8,
    base_qindex: u16,
    regular_quant_params: Av2RegularQuantParams,
    #[cfg(feature = "av2-lossy-stats")]
    stats: Option<std::cell::RefCell<Av2LossyStats>>,
}

#[derive(Clone, Copy)]
struct Av2LossyLeafPredictorContext<'a> {
    leaf_x0: usize,
    leaf_y0: usize,
    leaf_width: usize,
    leaf_height: usize,
    coded_mi_context: &'a Av2CodedMiContext,
}

// AVM uses its reciprocal-division table for intra DC predictors.  The
// approximation is observable for non-power-of-two edge counts (for example,
// 2156 / 12 rounds to 179 with AVM's multiplier, while integer division gives
// 180), so use the same small-count entries for encoder reconstruction.
#[inline]
pub(super) fn av2_reference_dc_average(sum: u32, count: u32) -> Av2Sample {
    let (scale, shift) = match count {
        1 => (512u32, 9u8),
        2 => (512, 10),
        3 => (341, 10),
        4 => (512, 11),
        5 => (410, 11),
        6 => (341, 11),
        7 => (293, 11),
        8 => (512, 12),
        9 => (455, 12),
        10 => (410, 12),
        11 => (372, 12),
        12 => (341, 12),
        13 => (315, 12),
        14 => (293, 12),
        15 => (273, 12),
        16 => (512, 13),
        _ => unreachable!("AV2 intra DC edge count exceeds supported block size"),
    };
    ((u64::from(sum) * u64::from(scale) + (1u64 << (shift - 1))) >> shift) as Av2Sample
}

impl<'a> Av2LossySubsampledTileState<'a> {
    fn new(
        geometry: Av2VideoGeometry,
        region: Av2TileRegion,
        chroma_format: Av2ChromaFormat,
        bit_depth: SampleBitDepth,
        source: &'a [u8],
        recon: &'a mut [u8],
        qp: u8,
        base_qindex: u16,
    ) -> Self {
        assert!(qp > 0, "AV2 lossy QP must be non-zero");
        assert!(base_qindex > 0, "AV2 regular lossy qindex must be non-zero");
        let layout =
            Av2PlanarTileLayout::for_validated_shape(geometry, region, chroma_format, bit_depth);
        let expected_len = layout.frame_len();
        assert_eq!(
            source.len(),
            expected_len,
            "AV2 planar lossy residual source length must match geometry"
        );
        assert_eq!(
            recon.len(),
            source.len(),
            "AV2 planar lossy residual reconstruction length must match source"
        );
        Self {
            #[cfg(feature = "av2-lossy-stats")]
            region,
            chroma_format,
            bit_depth,
            source,
            recon,
            layout,
            #[cfg(feature = "av2-lossy-stats")]
            qp,
            base_qindex,
            regular_quant_params: Av2RegularQuantParams::new(base_qindex, bit_depth),
            #[cfg(feature = "av2-lossy-stats")]
            stats: av2_lossy_stats_enabled()
                .then(|| std::cell::RefCell::new(Av2LossyStats::default())),
        }
    }

    fn plane_geometry(&self, plane: Av2LossyPlane) -> (usize, usize) {
        self.layout.plane_geometry(plane.planar())
    }

    fn plane_origin(&self, plane: Av2LossyPlane) -> (usize, usize) {
        self.layout.plane_origin(plane.planar())
    }

    fn plane_region_limit(&self, plane: Av2LossyPlane) -> (usize, usize) {
        self.layout.plane_region_limit(plane.planar())
    }

    fn plane_subsampling(&self, plane: Av2LossyPlane) -> (usize, usize) {
        self.layout.plane_subsampling(plane.planar())
    }

    fn coded_mi_for_plane_sample(&self, plane: Av2LossyPlane, x: usize, y: usize) -> (usize, usize) {
        self.layout
            .coded_mi_for_plane_sample(plane.planar(), x, y)
    }

    fn txb_origin(&self, plane: Av2LossyPlane, col: usize, row: usize) -> (usize, usize) {
        self.layout.txb_origin(plane.planar(), col, row)
    }

    fn offset(&self, plane: Av2LossyPlane, x: usize, y: usize) -> usize {
        self.layout.offset(plane.planar(), x, y)
    }

    fn source_sample(&self, plane: Av2LossyPlane, x: usize, y: usize) -> Av2Sample {
        self.read_sample(self.source, self.offset(plane, x, y))
    }

    fn reference_sample(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        x: usize,
        y: usize,
    ) -> Av2Sample {
        self.read_sample(reference, self.offset(plane, x, y))
    }

    fn recon_sample(&self, plane: Av2LossyPlane, x: usize, y: usize) -> Av2Sample {
        self.read_sample(self.recon, self.offset(plane, x, y))
    }

    fn set_recon_sample(&mut self, plane: Av2LossyPlane, x: usize, y: usize, sample: Av2Sample) {
        let offset = self.offset(plane, x, y);
        write_planar_sample(self.recon, offset, sample, self.bit_depth);
    }

    #[inline(always)]
    fn read_sample(&self, input: &[u8], sample_index: usize) -> Av2Sample {
        read_planar_sample(input, sample_index, self.bit_depth)
    }

    fn dc_predictor(&self, plane: Av2LossyPlane, x0: usize, y0: usize) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        if !have_left && !have_top {
            return av2_lossless_dc_predictor(self.bit_depth);
        }

        let mut sum = 0u32;
        let mut count = 0u32;
        if have_top {
            for x in x0..(x0 + TX4X4_SIZE) {
                sum += u32::from(self.recon_sample(plane, x, y0 - 1));
                count += 1;
            }
        }
        if have_left {
            for y in y0..(y0 + TX4X4_SIZE) {
                sum += u32::from(self.recon_sample(plane, x0 - 1, y));
                count += 1;
            }
        }
        av2_reference_dc_average(sum, count)
    }

    fn h_predictor(&self, plane: Av2LossyPlane, x0: usize, y0: usize, local_y: usize) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        if x0 > tile_origin_x {
            self.recon_sample(plane, x0 - 1, y0 + local_y)
        } else if y0 > tile_origin_y {
            self.recon_sample(plane, x0, y0 - 1)
        } else {
            av2_lossless_h_pred_left_edge(self.bit_depth)
        }
    }

    fn v_predictor(&self, plane: Av2LossyPlane, x0: usize, y0: usize, local_x: usize) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        if y0 > tile_origin_y {
            self.recon_sample(plane, x0 + local_x, y0 - 1)
        } else if x0 > tile_origin_x {
            self.recon_sample(plane, x0 - 1, y0)
        } else {
            av2_lossless_v_pred_above_edge(self.bit_depth)
        }
    }

    fn above_left_predictor(&self, plane: Av2LossyPlane, x0: usize, y0: usize) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        if have_left && have_top {
            self.recon_sample(plane, x0 - 1, y0 - 1)
        } else if have_top {
            self.recon_sample(plane, x0, y0 - 1)
        } else if have_left {
            self.recon_sample(plane, x0 - 1, y0)
        } else {
            av2_lossless_dc_predictor(self.bit_depth)
        }
    }

    fn analyze_txb(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        mode: Av2LossySubsampledModeDecision,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2LossyTxbAnalysis {
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let predictor_mode = match plane {
            Av2LossyPlane::Y => chroma_mode_for_luma_mode(mode.luma_intra_mode),
            Av2LossyPlane::U | Av2LossyPlane::V => mode.chroma_intra_mode,
        };
        let dc_pred = if predictor_mode == Av2ChromaIntraMode::Dc {
            self.dc_predictor(plane, x0, y0)
        } else {
            0
        };
        let mut h_pred = [0; TX4X4_SIZE];
        if matches!(
            predictor_mode,
            Av2ChromaIntraMode::Horizontal | Av2ChromaIntraMode::Paeth
        ) {
            for (local_y, pred) in h_pred.iter_mut().enumerate() {
                *pred = self.h_predictor(plane, x0, y0, local_y);
            }
        }
        let mut v_pred = [0; TX4X4_SIZE];
        if matches!(
            predictor_mode,
            Av2ChromaIntraMode::Vertical | Av2ChromaIntraMode::Paeth
        ) {
            for (local_x, pred) in v_pred.iter_mut().enumerate() {
                *pred = self.v_predictor(plane, x0, y0, local_x);
            }
        }
        let above_left = if predictor_mode == Av2ChromaIntraMode::Paeth {
            self.above_left_predictor(plane, x0, y0)
        } else {
            0
        };
        let smooth_edges = matches!(
            predictor_mode,
            Av2ChromaIntraMode::Smooth
                | Av2ChromaIntraMode::SmoothVertical
                | Av2ChromaIntraMode::SmoothHorizontal
        )
        .then(|| self.smooth_edges(plane, x0, y0, context));
        let luma_directional_angle = (plane == Av2LossyPlane::Y)
            .then(|| lossy_luma_idif_angle(mode.luma_intra_mode))
            .flatten();
        let luma_directional_predictor_state = luma_directional_angle.map(|angle| {
            let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
            let (constant, edges) =
                self.luma_directional_idif_predictor_state_with(
                    plane,
                    x0,
                    y0,
                    angle,
                    context,
                    &edge_sample,
                );
            (angle, constant, edges)
        });
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let predictor_sample =
                    if let Some((angle, constant, edges)) = luma_directional_predictor_state {
                        constant.unwrap_or_else(|| {
                            luma_directional_idif_predictor(
                                angle,
                                edges.expect("IDIF edges are precomputed"),
                                local_x,
                                local_y,
                                self.bit_depth,
                            )
                        })
                    } else {
                        match predictor_mode {
                            Av2ChromaIntraMode::Dc => dc_pred,
                            Av2ChromaIntraMode::Horizontal => h_pred[local_y],
                            Av2ChromaIntraMode::Vertical => v_pred[local_x],
                            Av2ChromaIntraMode::Paeth => {
                                paeth_predictor(h_pred[local_y], v_pred[local_x], above_left)
                            }
                            Av2ChromaIntraMode::Smooth
                            | Av2ChromaIntraMode::SmoothVertical
                            | Av2ChromaIntraMode::SmoothHorizontal => {
                                let (above, left) =
                                    smooth_edges.expect("smooth edges are precomputed");
                                let (smooth, smooth_v, smooth_h) =
                                    av2_highbd_smooth_intra_predictor_set(
                                        above,
                                        left,
                                        local_x,
                                        local_y,
                                        self.bit_depth,
                                    );
                                match predictor_mode {
                                    Av2ChromaIntraMode::Smooth => smooth,
                                    Av2ChromaIntraMode::SmoothVertical => smooth_v,
                                    Av2ChromaIntraMode::SmoothHorizontal => smooth_h,
                                    _ => {
                                        unreachable!("smooth predictor branch only handles smooth modes")
                                    }
                                }
                            }
                            _ => unreachable!(
                                "AV2 lossy mode search selects DC, H, V, Paeth, smooth, or luma IDIF"
                            ),
                        }
                    };
                let source_sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                source[index] = source_sample;
                predictor[index] = predictor_sample;
            }
        }
        self.finish_txb_analysis(source, predictor)
    }

    fn analyze_inter_txb(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> Av2LossyTxbAnalysis {
        assert_eq!(
            reference.len(),
            self.source.len(),
            "AV2 lossy inter reference length must match source"
        );
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        debug_assert_eq!(usize::from(mv_col_px.unsigned_abs()) % sub_x, 0);
        debug_assert_eq!(usize::from(mv_row_px.unsigned_abs()) % sub_y, 0);
        let ref_x0 = x0 as isize + isize::from(mv_col_px) / sub_x as isize;
        let ref_y0 = y0 as isize + isize::from(mv_row_px) / sub_y as isize;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        assert!(
            ref_x0 >= 0 && ref_y0 >= 0,
            "AV2 lossy inter reference is out of bounds"
        );
        let ref_x0 = ref_x0 as usize;
        let ref_y0 = ref_y0 as usize;
        assert!(
            ref_x0 + TX4X4_SIZE <= plane_width && ref_y0 + TX4X4_SIZE <= plane_height,
            "AV2 lossy inter reference is out of bounds"
        );

        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let source_sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                let predictor_sample =
                    self.reference_sample(reference, plane, ref_x0 + local_x, ref_y0 + local_y);
                source[index] = source_sample;
                predictor[index] = predictor_sample;
            }
        }
        self.finish_txb_analysis(source, predictor)
    }

    fn finish_txb_analysis(
        &self,
        source: [Av2Sample; TX4X4_SAMPLES],
        predictor: [Av2Sample; TX4X4_SAMPLES],
    ) -> Av2LossyTxbAnalysis {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let mut sum = 0i32;
        for index in 0..TX4X4_SAMPLES {
            let diff = i32::from(source[index]) - i32::from(predictor[index]);
            residual[index] = diff;
            sum += diff;
        }
        let average = round_div_i32(sum, TX4X4_SAMPLES as i32);
        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(average, lossy_dc_delta_quant_step(self.quant_step()))
            .clamp(-max_delta, max_delta) as i16;
        let source_variance = txb_source_variance(&source);
        let (dc_sse, dc_variance_loss) = txb_dc_recon_distortion_with_source_variance(
            &source,
            &predictor,
            delta,
            self.bit_depth,
            source_variance,
        );
        Av2LossyTxbAnalysis {
            source,
            predictor,
            residual,
            delta,
            dc_sse,
            dc_variance_loss,
            source_variance,
        }
    }

    fn fill_quantized_recon_txb(
        &mut self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        analysis: &Av2LossyTxbAnalysis,
    ) {
        let (plane_width, plane_height) = self.plane_geometry(plane);
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            if y >= plane_height {
                continue;
            }
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                if x < plane_width {
                    let index = local_y * TX4X4_SIZE + local_x;
                    let predictor = i32::from(analysis.predictor[index]);
                    let sample = (predictor + i32::from(analysis.delta))
                        .clamp(0, i32::from(self.bit_depth.max_sample()))
                        as Av2Sample;
                    self.set_recon_sample(plane, x, y, sample);
                }
            }
        }
    }

    fn fill_residual_recon_txb(
        &mut self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        analysis: &Av2LossyTxbAnalysis,
        residual: &[i32; TX4X4_SAMPLES],
    ) {
        let (plane_width, plane_height) = self.plane_geometry(plane);
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            if y >= plane_height {
                continue;
            }
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                if x < plane_width {
                    let index = local_y * TX4X4_SIZE + local_x;
                    let predictor = i32::from(analysis.predictor[index]);
                    let sample = (predictor + residual[index])
                        .clamp(0, i32::from(self.bit_depth.max_sample()))
                        as Av2Sample;
                    self.set_recon_sample(plane, x, y, sample);
                }
            }
        }
    }

    fn fill_dpcm_residual_recon_txb(
        &mut self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        analysis: &Av2LossyTxbAnalysis,
        residual: &[i32; TX4X4_SAMPLES],
        horz: bool,
    ) {
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let (recon_samples, _) = dpcm_recon_samples_and_sse(
            analysis,
            residual,
            horz,
            i32::from(self.bit_depth.max_sample()),
        );
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            if y >= plane_height {
                continue;
            }
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                if x < plane_width {
                    let index = local_y * TX4X4_SIZE + local_x;
                    self.set_recon_sample(plane, x, y, recon_samples[index] as Av2Sample);
                }
            }
        }
    }

    fn copy_source_to_recon_txb(
        &mut self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        analysis: &Av2LossyTxbAnalysis,
    ) {
        let (plane_width, plane_height) = self.plane_geometry(plane);
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            if y >= plane_height {
                continue;
            }
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                if x < plane_width {
                    let index = local_y * TX4X4_SIZE + local_x;
                    self.set_recon_sample(plane, x, y, analysis.source[index]);
                }
            }
        }
    }

    fn chroma_444_intra_tx8x8_analysis(
        &self,
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mode: Av2LossySubsampledModeDecision,
    ) -> Av2LossyTx8x8Analysis {
        let plan = self.chroma_444_intra_predictor_plan(plane, chroma_span, mode);
        self.chroma_444_tx8x8_analysis(
            plane,
            plan.leaf_x0,
            plan.leaf_y0,
            plan.visible_width,
            plan.visible_height,
            |state, local_x, local_y| plan.sample(state, plane, local_x, local_y),
        )
    }

    fn fill_chroma_444_tx8x8_leaf(
        &mut self,
        plane: Av2LossyPlane,
        analysis: &Av2LossyTx8x8Analysis,
        residual: &[i32; TX8X8_SAMPLES],
    ) {
        let max_sample = i32::from(self.bit_depth.max_sample());
        for local_y in 0..analysis.visible_height {
            let y = analysis.leaf_y0 + local_y;
            for local_x in 0..analysis.visible_width {
                let x = analysis.leaf_x0 + local_x;
                let index = local_y * TX8X8_SIZE + local_x;
                let predictor = i32::from(analysis.predictor[index]);
                let sample = (predictor + residual[index]).clamp(0, max_sample) as Av2Sample;
                self.set_recon_sample(plane, x, y, sample);
            }
        }
    }

    fn fill_chroma_422_tx4x8_leaf(
        &mut self,
        plane: Av2LossyPlane,
        analysis: &Av2LossyTx8x8Analysis,
        residual: &[i32; TX4X8_SAMPLES],
    ) {
        let max_sample = i32::from(self.bit_depth.max_sample());
        debug_assert!(analysis.visible_width <= TX4X8_WIDTH);
        debug_assert!(analysis.visible_height <= TX4X8_HEIGHT);
        for local_y in 0..analysis.visible_height {
            let y = analysis.leaf_y0 + local_y;
            for local_x in 0..analysis.visible_width {
                let x = analysis.leaf_x0 + local_x;
                let analysis_index = local_y * TX8X8_SIZE + local_x;
                let residual_index = local_y * TX4X8_WIDTH + local_x;
                let predictor = i32::from(analysis.predictor[analysis_index]);
                let sample = (predictor + residual[residual_index]).clamp(0, max_sample) as Av2Sample;
                self.set_recon_sample(plane, x, y, sample);
            }
        }
    }

    fn chroma_444_intra_predictor_plan(
        &self,
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mode: Av2LossySubsampledModeDecision,
    ) -> Av2Chroma444IntraPredictorPlan {
        debug_assert!(matches!(plane, Av2LossyPlane::U | Av2LossyPlane::V));
        let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
        let width = chroma_span.width * TX4X4_SIZE;
        let height = chroma_span.height * TX4X4_SIZE;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let visible_width = width.min(plane_width.saturating_sub(leaf_x0)).min(TX8X8_SIZE);
        let visible_height = height
            .min(plane_height.saturating_sub(leaf_y0))
            .min(TX8X8_SIZE);
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = leaf_x0 > tile_origin_x;
        let have_top = leaf_y0 > tile_origin_y;
        let mode = mode.chroma_intra_mode;

        let dc_pred = if mode == Av2ChromaIntraMode::Dc {
            if !have_left && !have_top {
                av2_lossless_dc_predictor(self.bit_depth)
            } else {
                let mut sum = 0u32;
                let mut count = 0u32;
                if have_top {
                    for x in leaf_x0..(leaf_x0 + visible_width) {
                        let sample = self.recon_sample(plane, x, leaf_y0 - 1);
                        sum += u32::from(sample);
                        count += 1;
                    }
                }
                if have_left {
                    for y in leaf_y0..(leaf_y0 + visible_height) {
                        let sample = self.recon_sample(plane, leaf_x0 - 1, y);
                        sum += u32::from(sample);
                        count += 1;
                    }
                }
                if count == 0 {
                    av2_lossless_dc_predictor(self.bit_depth)
                } else {
                    av2_reference_dc_average(sum, count)
                }
            }
        } else {
            0
        };
        let above_left = if mode == Av2ChromaIntraMode::Paeth {
            if have_left && have_top {
                self.recon_sample(plane, leaf_x0 - 1, leaf_y0 - 1)
            } else if have_top {
                self.recon_sample(plane, leaf_x0, leaf_y0 - 1)
            } else if have_left {
                self.recon_sample(plane, leaf_x0 - 1, leaf_y0)
            } else {
                av2_lossless_dc_predictor(self.bit_depth)
            }
        } else {
            0
        };

        Av2Chroma444IntraPredictorPlan {
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            have_left,
            have_top,
            dc_pred,
            above_left,
            mode,
            bit_depth: self.bit_depth,
        }
    }

    fn chroma_444_inter_tx8x8_analysis(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> Av2LossyTx8x8Analysis {
        let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
        let (visible_width, visible_height) =
            self.chroma_444_leaf_visible_size(plane, leaf_x0, leaf_y0, chroma_span);
        let (ref_x0, ref_y0) = self.chroma_444_inter_reference_origin(
            reference,
            plane,
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            mv_row_px,
            mv_col_px,
        );
        self.chroma_444_tx8x8_analysis(
            plane,
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            |state, local_x, local_y| {
                state.reference_sample(reference, plane, ref_x0 + local_x, ref_y0 + local_y)
            },
        )
    }

    fn chroma_444_leaf_visible_size(
        &self,
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        chroma_span: Av2ChromaTx4x4Span,
    ) -> (usize, usize) {
        let width = (chroma_span.width * TX4X4_SIZE).min(TX8X8_SIZE);
        let height = (chroma_span.height * TX4X4_SIZE).min(TX8X8_SIZE);
        let (plane_width, plane_height) = self.plane_geometry(plane);
        (
            width.min(plane_width.saturating_sub(leaf_x0)),
            height.min(plane_height.saturating_sub(leaf_y0)),
        )
    }

    fn chroma_444_inter_reference_origin(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        visible_width: usize,
        visible_height: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> (usize, usize) {
        assert_eq!(
            reference.len(),
            self.source.len(),
            "AV2 lossy inter reference length must match source"
        );
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        debug_assert_eq!(usize::from(mv_col_px.unsigned_abs()) % sub_x, 0);
        debug_assert_eq!(usize::from(mv_row_px.unsigned_abs()) % sub_y, 0);
        let ref_x0 = leaf_x0 as isize + isize::from(mv_col_px) / sub_x as isize;
        let ref_y0 = leaf_y0 as isize + isize::from(mv_row_px) / sub_y as isize;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        assert!(
            ref_x0 >= 0 && ref_y0 >= 0,
            "AV2 lossy inter reference is out of bounds"
        );
        let ref_x0 = ref_x0 as usize;
        let ref_y0 = ref_y0 as usize;
        assert!(
            ref_x0 + visible_width <= plane_width && ref_y0 + visible_height <= plane_height,
            "AV2 lossy inter reference is out of bounds"
        );
        (ref_x0, ref_y0)
    }

    fn chroma_444_tx8x8_analysis<Predictor>(
        &self,
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        visible_width: usize,
        visible_height: usize,
        predictor: Predictor,
    ) -> Av2LossyTx8x8Analysis
    where
        Predictor: Fn(&Self, usize, usize) -> Av2Sample,
    {
        debug_assert!(matches!(plane, Av2LossyPlane::U | Av2LossyPlane::V));
        let mut predictor_block = [0; TX8X8_SAMPLES];
        let mut residual = [0i32; TX8X8_SAMPLES];
        for local_y in 0..TX8X8_SIZE {
            for local_x in 0..TX8X8_SIZE {
                let index = local_y * TX8X8_SIZE + local_x;
                if local_x < visible_width && local_y < visible_height {
                    let pred = predictor(self, local_x, local_y);
                    let source = self.source_sample(plane, leaf_x0 + local_x, leaf_y0 + local_y);
                    predictor_block[index] = pred;
                    residual[index] = i32::from(source) - i32::from(pred);
                }
            }
        }

        Av2LossyTx8x8Analysis {
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            predictor: predictor_block,
            residual,
        }
    }

    fn regular_dct_quantized_tx8x8_residual_candidate(
        &self,
        analysis: &Av2LossyTx8x8Analysis,
    ) -> Av2LossyTx8x8QuantizedResidualCandidate {
        if analysis.residual.iter().all(|&residual| residual == 0) {
            // An exact predictor match has an all-zero regular-DCT result;
            // preserve the ordinary candidate shape without running FDCT or
            // IDCT on the 8x8 block.
            return Av2LossyTx8x8QuantizedResidualCandidate {
                coefficients: [0; TX8X8_SAMPLES],
                residual: [0; TX8X8_SAMPLES],
            };
        }
        let coefficients = av2_fdct8x8(&analysis.residual);
        let (qcoeff, dqcoeff) =
            av2_regular_quantize_dct8x8_with_params(&coefficients, self.regular_quant_params);
        let residual = if dqcoeff[1..]
            .iter()
            .all(|&coefficient| coefficient == 0)
        {
            av2_idct8x8_dc_only(&dqcoeff, self.bit_depth)
        } else {
            av2_idct8x8(&dqcoeff, self.bit_depth)
        };
        Av2LossyTx8x8QuantizedResidualCandidate {
            coefficients: av2_regular_quantized_level_coefficients_tx8x8(&qcoeff),
            residual,
        }
    }

    fn regular_dct_quantized_tx4x8_residual_candidate(
        &self,
        analysis: &Av2LossyTx8x8Analysis,
    ) -> Av2LossyTx4x8QuantizedResidualCandidate {
        debug_assert!(analysis.visible_width <= TX4X8_WIDTH);
        debug_assert!(analysis.visible_height <= TX4X8_HEIGHT);
        if analysis.residual.iter().all(|&residual| residual == 0) {
            // The unused right-hand columns are already zero, so an exact
            // predictor match also has an all-zero TX_4X8 candidate.
            return Av2LossyTx4x8QuantizedResidualCandidate {
                coefficients: [0; TX4X8_SAMPLES],
                residual: [0; TX4X8_SAMPLES],
            };
        }
        let mut residual = [0i32; TX4X8_SAMPLES];
        for local_y in 0..TX4X8_HEIGHT {
            for local_x in 0..TX4X8_WIDTH {
                residual[local_y * TX4X8_WIDTH + local_x] =
                    analysis.residual[local_y * TX8X8_SIZE + local_x];
            }
        }
        let coefficients = av2_fdct4x8(&residual);
        let (qcoeff, dqcoeff) =
            av2_regular_quantize_dct4x8_with_params(&coefficients, self.regular_quant_params);
        Av2LossyTx4x8QuantizedResidualCandidate {
            coefficients: av2_regular_quantized_level_coefficients_tx4x8(&qcoeff),
            residual: av2_idct4x8(&dqcoeff, self.bit_depth),
        }
    }

    fn quantized_residual_candidate(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        use_fsc: bool,
    ) -> Av2LossyQuantizedResidualCandidate {
        self.quantized_residual_candidate_with_step(analysis, self.quant_step(), use_fsc)
    }

    fn refined_quantized_residual_candidate(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        use_fsc: bool,
    ) -> Av2LossyQuantizedResidualCandidate {
        self.quantized_residual_candidate_with_step(
            analysis,
            refined_lossy_quant_step(self.quant_step()),
            use_fsc,
        )
    }

    fn transform_quantized_residual_candidate(
        &self,
        analysis: &Av2LossyTxbAnalysis,
    ) -> Av2LossyQuantizedResidualCandidate {
        let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, false);
        let coeff_step = lossy_transform_coeff_step(self.quant_step());
        let mut quantized_coefficients = [0i32; TX4X4_SAMPLES];
        for (dst, coefficient) in quantized_coefficients.iter_mut().zip(coefficients) {
            *dst = quantize_i32_to_step(coefficient, coeff_step);
        }
        let residual = av2_iwht4x4(&quantized_coefficients);
        let max_sample = i32::from(self.bit_depth.max_sample());
        let (sse, variance_loss) = txb_recon_sse_and_variance_loss(
            &analysis.source,
            &analysis.predictor,
            &residual,
            max_sample,
            analysis.source_variance,
        );
        Av2LossyQuantizedResidualCandidate {
            kind: Av2LossyResidualCandidateKind::Transform,
            residual,
            coefficients: quantized_coefficients,
            sse,
            variance_loss,
        }
    }

    fn regular_dct_quantized_residual_candidates(
        &self,
        analysis: &Av2LossyTxbAnalysis,
    ) -> Av2LossyRegularDctCandidates {
        if analysis.residual.iter().all(|&residual| residual == 0) {
            // A predictor-exact TXB cannot produce any coded coefficient. Keep
            // the regular-DCT kind and its zero candidate so the syntax path
            // and strict tie behavior remain identical without running the
            // transform and inverse transform.
            let zero = Av2LossyQuantizedResidualCandidate {
                kind: Av2LossyResidualCandidateKind::RegularDct,
                residual: [0; TX4X4_SAMPLES],
                coefficients: [0; TX4X4_SAMPLES],
                sse: 0,
                variance_loss: 0,
            };
            return Av2LossyRegularDctCandidates {
                transform: zero,
                tail_pruned: None,
                double_tail_pruned: None,
                dc_only: zero,
            };
        }
        let coefficients = av2_fdct4x4(&analysis.residual);
        let (mut qcoeff, _) =
            av2_regular_quantize_dct4x4_with_params(&coefficients, self.regular_quant_params);
        prune_regular_dct_ac_levels(
            &mut qcoeff,
            self.base_qindex,
            self.bit_depth,
            self.chroma_format,
            analysis.source_variance,
        );
        let transform = self.regular_dct_candidate_from_qcoeff(analysis, &qcoeff);
        if qcoeff[1..].iter().all(|&coefficient| coefficient == 0) {
            // The tail-pruned and DC-only candidates would reconstruct and
            // score identically here. Keep the regular-DCT candidate so the
            // existing strict tie break and syntax path remain unchanged.
            return Av2LossyRegularDctCandidates {
                transform,
                tail_pruned: None,
                double_tail_pruned: None,
                dc_only: transform,
            };
        }
        let mut tail_pruned_qcoeff = qcoeff;
        let tail_pruned = (prune_regular_dct_trailing_unit_acs(&mut tail_pruned_qcoeff, 1) == 1)
            .then(|| {
                self.regular_dct_candidate_from_qcoeff_kind(
                    analysis,
                    &tail_pruned_qcoeff,
                    Av2LossyResidualCandidateKind::RegularDctTailPruned,
                )
            });
        let mut double_tail_pruned_qcoeff = qcoeff;
        let double_tail_pruned = ((self.chroma_format != Av2ChromaFormat::Yuv444
            || self.bit_depth.bits() > 8)
            && prune_regular_dct_trailing_unit_acs(&mut double_tail_pruned_qcoeff, 2) == 2)
            .then(|| {
                self.regular_dct_candidate_from_qcoeff_kind(
                    analysis,
                    &double_tail_pruned_qcoeff,
                    Av2LossyResidualCandidateKind::RegularDctDoubleTailPruned,
                )
            });
        let mut dc_only_qcoeff = qcoeff;
        dc_only_qcoeff[1..].fill(0);
        let dc_only = self.regular_dct_candidate_from_qcoeff_kind(
            analysis,
            &dc_only_qcoeff,
            Av2LossyResidualCandidateKind::RegularDctDcOnly,
        );
        Av2LossyRegularDctCandidates {
            transform,
            tail_pruned,
            double_tail_pruned,
            dc_only,
        }
    }

    fn regular_dct_candidate_from_qcoeff(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        qcoeff: &[i32; TX4X4_SAMPLES],
    ) -> Av2LossyQuantizedResidualCandidate {
        self.regular_dct_candidate_from_qcoeff_kind(
            analysis,
            qcoeff,
            Av2LossyResidualCandidateKind::RegularDct,
        )
    }

    fn regular_dct_candidate_from_qcoeff_kind(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        qcoeff: &[i32; TX4X4_SAMPLES],
        kind: Av2LossyResidualCandidateKind,
    ) -> Av2LossyQuantizedResidualCandidate {
        let dqcoeff = av2_regular_dequantize(
            qcoeff,
            av2_regular_dequant_qtx(self.base_qindex, self.bit_depth),
        );
        let residual = if dqcoeff[1..]
            .iter()
            .all(|&coefficient| coefficient == 0)
        {
            av2_idct4x4_dc_only(&dqcoeff, self.bit_depth)
        } else {
            av2_idct4x4(&dqcoeff, self.bit_depth)
        };
        let max_sample = i32::from(self.bit_depth.max_sample());
        let (sse, variance_loss) = txb_recon_sse_and_variance_loss(
            &analysis.source,
            &analysis.predictor,
            &residual,
            max_sample,
            analysis.source_variance,
        );
        Av2LossyQuantizedResidualCandidate {
            kind,
            residual,
            coefficients: av2_regular_quantized_level_coefficients(qcoeff),
            sse,
            variance_loss,
        }
    }

    fn analyze_dpcm_txb(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        horz: bool,
    ) -> Av2LossyTxbAnalysis {
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let mut residual = [0i32; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                let pred = if horz {
                    if local_x == 0 {
                        self.h_predictor(plane, x0, y0, local_y)
                    } else {
                        source[index - 1]
                    }
                } else if local_y == 0 {
                    self.v_predictor(plane, x0, y0, local_x)
                } else {
                    source[index - TX4X4_SIZE]
                };
                source[index] = sample;
                predictor[index] = pred;
                residual[index] = i32::from(sample) - i32::from(pred);
            }
        }
        let source_variance = txb_source_variance(&source);
        Av2LossyTxbAnalysis {
            source,
            predictor,
            residual,
            delta: 0,
            dc_sse: 0,
            dc_variance_loss: 0,
            source_variance,
        }
    }

    fn quantized_dpcm_residual_candidate(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        step: i32,
        horz: bool,
    ) -> Av2LossyQuantizedResidualCandidate {
        let mut residual = [0i32; TX4X4_SAMPLES];
        for (dst, &sample) in residual.iter_mut().zip(analysis.residual.iter()) {
            *dst = quantize_i32_to_step(sample, step);
        }
        self.dpcm_candidate_from_residual(
            analysis,
            residual,
            horz,
            if step < self.quant_step() {
                Av2LossyResidualCandidateKind::RefinedSpatial
            } else {
                Av2LossyResidualCandidateKind::Spatial
            },
        )
    }

    fn transform_quantized_dpcm_residual_candidate(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        horz: bool,
    ) -> Av2LossyQuantizedResidualCandidate {
        let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, false);
        let coeff_step = lossy_transform_coeff_step(self.quant_step());
        let mut quantized_coefficients = [0i32; TX4X4_SAMPLES];
        for (dst, coefficient) in quantized_coefficients.iter_mut().zip(coefficients) {
            *dst = quantize_i32_to_step(coefficient, coeff_step);
        }
        let residual = av2_iwht4x4(&quantized_coefficients);
        self.dpcm_candidate_from_residual(
            analysis,
            residual,
            horz,
            Av2LossyResidualCandidateKind::Transform,
        )
    }

    fn dpcm_candidate_from_residual(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        residual: [i32; TX4X4_SAMPLES],
        horz: bool,
        kind: Av2LossyResidualCandidateKind,
    ) -> Av2LossyQuantizedResidualCandidate {
        let (recon_samples, sse) = dpcm_recon_samples_and_sse(
            analysis,
            &residual,
            horz,
            i32::from(self.bit_depth.max_sample()),
        );
        Av2LossyQuantizedResidualCandidate {
            kind,
            residual,
            coefficients: tx4x4_coefficients_from_residual(&residual, false),
            sse,
            variance_loss: txb_recon_variance_loss(analysis.source_variance, &recon_samples),
        }
    }

    fn quantized_residual_candidate_with_step(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        step: i32,
        use_fsc: bool,
    ) -> Av2LossyQuantizedResidualCandidate {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let predictor = i32::from(analysis.predictor[index]);
            let quantized = quantize_i32_to_step(analysis.residual[index], step)
                .clamp(-predictor, max_sample - predictor);
            residual[index] = quantized;
        }
        let (sse, variance_loss) = txb_recon_sse_and_variance_loss(
            &analysis.source,
            &analysis.predictor,
            &residual,
            max_sample,
            analysis.source_variance,
        );
        Av2LossyQuantizedResidualCandidate {
            kind: if step < self.quant_step() {
                Av2LossyResidualCandidateKind::RefinedSpatial
            } else {
                Av2LossyResidualCandidateKind::Spatial
            },
            coefficients: tx4x4_coefficients_from_residual(&residual, use_fsc),
            residual,
            sse,
            variance_loss,
        }
    }

    fn mode_decision_for_leaf(
        &self,
        decision: Av2TileDecision,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
        coded_mi_context: &Av2CodedMiContext,
        luma_mode_syntax: Av2LumaModeSyntax,
    ) -> Av2LossySubsampledModeDecision {
        let txb_width = decision
            .block_size
            .tx4x4_width()
            .min(visible_cols_mi.saturating_sub(decision.col));
        let txb_height = decision
            .block_size
            .tx4x4_height()
            .min(visible_rows_mi.saturating_sub(decision.row));
        let (luma_leaf_x0, luma_leaf_y0) =
            self.txb_origin(Av2LossyPlane::Y, decision.col, decision.row);
        let luma_leaf_width = txb_width * TX4X4_SIZE;
        let luma_leaf_height = txb_height * TX4X4_SIZE;
        let luma_context = Av2LossyLeafPredictorContext {
            leaf_x0: luma_leaf_x0,
            leaf_y0: luma_leaf_y0,
            leaf_width: luma_leaf_width,
            leaf_height: luma_leaf_height,
            coded_mi_context,
        };
        let mut mode = Av2LossySubsampledModeDecision::default();
        let mut luma_scores = Av2LossyIntraTxbScores::default();
        let mut luma_sampled_txbs = 0usize;
        for row in 0..txb_height {
            for col in 0..txb_width {
                if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                    continue;
                }
                let (x0, y0) =
                    self.txb_origin(Av2LossyPlane::Y, decision.col + col, decision.row + row);
                luma_scores.add_assign(self.intra_txb_scores_for_score(
                    Av2LossyPlane::Y,
                    x0,
                    y0,
                    luma_context,
                    Av2CoefficientProxyKind::LumaTransform,
                    true,
                ));
                luma_sampled_txbs += 1;
            }
        }
        luma_scores = luma_scores.scaled_to_txb_count(txb_width * txb_height, luma_sampled_txbs);
        let luma_smooth_scores = lossy_luma_smooth_search_allowed(luma_scores, txb_width * txb_height)
            .then(|| {
                let mut smooth_scores = Av2LossyIntraTxbScores::default();
                let mut sampled_txbs = 0usize;
                for row in 0..txb_height {
                    for col in 0..txb_width {
                        if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                            continue;
                        }
                        let (x0, y0) = self.txb_origin(
                            Av2LossyPlane::Y,
                            decision.col + col,
                            decision.row + row,
                        );
                        smooth_scores.add_assign(self.smooth_txb_scores_for_score(
                            Av2LossyPlane::Y,
                            x0,
                            y0,
                            luma_context,
                            Av2CoefficientProxyKind::LumaTransform,
                        ));
                        sampled_txbs += 1;
                    }
                }
                smooth_scores.scaled_to_txb_count(txb_width * txb_height, sampled_txbs)
            });
        let luma_directional_scores =
            lossy_luma_directional_search_allowed(
                luma_scores,
                txb_width * txb_height,
                self.chroma_format,
                self.bit_depth,
            )
            .then(|| {
                let mut directional_scores = [
                    (Av2LumaIntraMode::Directional45, 0usize),
                    (Av2LumaIntraMode::Directional67, 0usize),
                    (Av2LumaIntraMode::Directional113, 0usize),
                    (Av2LumaIntraMode::Directional135, 0usize),
                    (Av2LumaIntraMode::Directional157, 0usize),
                    (Av2LumaIntraMode::Directional203, 0usize),
                ];
                let mut sampled_txbs = 0usize;
                for row in 0..txb_height {
                    for col in 0..txb_width {
                        if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                            continue;
                        }
                        let (x0, y0) = self.txb_origin(
                            Av2LossyPlane::Y,
                            decision.col + col,
                            decision.row + row,
                        );
                        for (luma_intra_mode, score) in directional_scores.iter_mut() {
                            *score += self.directional_txb_score_for_score(
                                x0,
                                y0,
                                luma_context,
                                Av2CoefficientProxyKind::LumaTransform,
                                *luma_intra_mode,
                            );
                        }
                        sampled_txbs += 1;
                    }
                }
                let total_txbs = txb_width * txb_height;
                if sampled_txbs != 0 && sampled_txbs != total_txbs {
                    for (_, score) in directional_scores.iter_mut() {
                        *score = score.saturating_mul(total_txbs) / sampled_txbs;
                    }
                }
                directional_scores
            });
        let luma_directional_delta_scores = luma_directional_scores.as_ref().map(|scores| {
            let best_base = scores
                .iter()
                .min_by_key(|(mode, score)| {
                    *score + lossy_luma_mode_syntax_penalty(*mode, luma_mode_syntax)
                })
                .map(|(mode, _)| *mode);
            let Some(best_base) = best_base else {
                return ([(Av2LumaIntraMode::Dc, 0usize); 2], 0);
            };
            let Some((base, _)) = best_base.directional() else {
                return ([(Av2LumaIntraMode::Dc, 0usize); 2], 0);
            };
            let mut delta_scores = [(Av2LumaIntraMode::Dc, 0usize); 2];
            let mut delta_count = 0usize;
            // The outermost pair is the useful lossy refinement on the
            // measured 4:2:0 content; avoid redundant inner-angle searches.
            for delta in [-3i8, 3] {
                let mode = Av2LumaIntraMode::DirectionalDelta { base, delta };
                let mut score = 192 + usize::from(delta.unsigned_abs()) * 16;
                let mut sampled_txbs = 0usize;
                for row in 0..txb_height {
                    for col in 0..txb_width {
                        if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                            continue;
                        }
                        let (x0, y0) = self.txb_origin(
                            Av2LossyPlane::Y,
                            decision.col + col,
                            decision.row + row,
                        );
                        score += self.directional_txb_score_for_score(
                            x0,
                            y0,
                            luma_context,
                            Av2CoefficientProxyKind::LumaTransform,
                            mode,
                        );
                        sampled_txbs += 1;
                    }
                }
                let total_txbs = txb_width * txb_height;
                if sampled_txbs != 0 && sampled_txbs != total_txbs {
                    score = score.saturating_mul(total_txbs) / sampled_txbs;
                }
                delta_scores[delta_count] = (mode, score);
                delta_count += 1;
            }
            (delta_scores, delta_count)
        });
        let mut best_luma = (mode.luma_intra_mode, mode.luma_bdpcm_horz, usize::MAX);
        for (luma_intra_mode, luma_bdpcm_horz, base_score, syntax_penalty) in [
            (
                Av2LumaIntraMode::Dc,
                None,
                luma_scores.dc,
                lossy_luma_mode_syntax_penalty(Av2LumaIntraMode::Dc, luma_mode_syntax),
            ),
            (
                Av2LumaIntraMode::Horizontal,
                None,
                luma_scores.horizontal,
                lossy_luma_mode_syntax_penalty(Av2LumaIntraMode::Horizontal, luma_mode_syntax),
            ),
            (
                Av2LumaIntraMode::Vertical,
                None,
                luma_scores.vertical,
                lossy_luma_mode_syntax_penalty(Av2LumaIntraMode::Vertical, luma_mode_syntax),
            ),
            (
                Av2LumaIntraMode::Paeth,
                None,
                luma_scores.paeth,
                lossy_luma_mode_syntax_penalty(Av2LumaIntraMode::Paeth, luma_mode_syntax),
            ),
        ] {
            let score = base_score + syntax_penalty;
            if score < best_luma.2 {
                best_luma = (luma_intra_mode, luma_bdpcm_horz, score);
            }
        }
        if let Some(smooth_scores) = luma_smooth_scores {
            for (luma_intra_mode, score) in [
                (Av2LumaIntraMode::Smooth, smooth_scores.smooth),
                (
                    Av2LumaIntraMode::SmoothVertical,
                    smooth_scores.smooth_vertical,
                ),
                (
                    Av2LumaIntraMode::SmoothHorizontal,
                    smooth_scores.smooth_horizontal,
                ),
            ] {
                let score = score + 192usize;
                if score < best_luma.2 {
                    best_luma = (luma_intra_mode, None, score);
                }
            }
        }
        if let Some(directional_scores) = luma_directional_scores {
            for (luma_intra_mode, score) in directional_scores {
                let score =
                    score + lossy_luma_mode_syntax_penalty(luma_intra_mode, luma_mode_syntax);
                if score < best_luma.2 {
                    best_luma = (luma_intra_mode, None, score);
                }
            }
        }
        if let Some((delta_scores, delta_count)) = luma_directional_delta_scores {
            for (luma_intra_mode, score) in delta_scores.into_iter().take(delta_count) {
                if score < best_luma.2 {
                    best_luma = (luma_intra_mode, None, score);
                }
            }
        }
        mode.luma_intra_mode = best_luma.0;
        mode.luma_bdpcm_horz = best_luma.1;
        if mode.luma_bdpcm_horz.is_none()
            && mode.luma_intra_mode != Av2LumaIntraMode::Dc
            && lossy_regular_q_dc_refinement_allowed(
                best_luma.2,
                luma_scores.dc + lossy_luma_mode_syntax_penalty(Av2LumaIntraMode::Dc, luma_mode_syntax),
                txb_width * txb_height,
            )
        {
            let selected_score = self.sampled_luma_regular_q_leaf_score(
                decision,
                txb_width,
                txb_height,
                luma_context,
                mode,
                luma_mode_syntax,
            );
            let dc_mode = Av2LossySubsampledModeDecision {
                luma_intra_mode: Av2LumaIntraMode::Dc,
                luma_bdpcm_horz: None,
                ..mode
            };
            let dc_score = self.sampled_luma_regular_q_leaf_score(
                decision,
                txb_width,
                txb_height,
                luma_context,
                dc_mode,
                luma_mode_syntax,
            );
            if lossy_regular_q_refinement_selects_dc(
                selected_score,
                dc_score,
                txb_width * txb_height,
                self.quant_step(),
            ) {
                mode.luma_intra_mode = Av2LumaIntraMode::Dc;
                mode.luma_bdpcm_horz = None;
            }
        }

        let chroma_span = chroma_tx4x4_span(
            decision,
            visible_rows_mi,
            visible_cols_mi,
            self.chroma_format,
        );
        let (chroma_leaf_x0, chroma_leaf_y0) =
            self.txb_origin(Av2LossyPlane::U, chroma_span.col, chroma_span.row);
        let chroma_leaf_width = chroma_span.width * TX4X4_SIZE;
        let chroma_leaf_height = chroma_span.height * TX4X4_SIZE;
        let chroma_context = Av2LossyLeafPredictorContext {
            leaf_x0: chroma_leaf_x0,
            leaf_y0: chroma_leaf_y0,
            leaf_width: chroma_leaf_width,
            leaf_height: chroma_leaf_height,
            coded_mi_context,
        };
        let mut chroma_scores = Av2LossyIntraTxbScores::default();
        let mut chroma_sampled_txbs = 0usize;
        for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
            for row in 0..chroma_span.height {
                for col in 0..chroma_span.width {
                    if !lossy_mode_search_samples_txb(
                        row,
                        col,
                        chroma_span.width,
                        chroma_span.height,
                    ) {
                        continue;
                    }
                    let (x0, y0) =
                        self.txb_origin(plane, chroma_span.col + col, chroma_span.row + row);
                    chroma_scores.add_assign(self.intra_txb_scores_for_score(
                        plane,
                        x0,
                        y0,
                        chroma_context,
                        Av2CoefficientProxyKind::ChromaTransform,
                        false,
                    ));
                    chroma_sampled_txbs += 1;
                }
            }
        }
        chroma_scores = chroma_scores.scaled_to_txb_count(
            chroma_span.width * chroma_span.height * 2,
            chroma_sampled_txbs,
        );
        let chroma_total_txbs = chroma_span.width * chroma_span.height * 2;
        let chroma_paeth_score =
            lossy_chroma_paeth_search_allowed(chroma_scores, chroma_total_txbs).then(|| {
                let mut paeth_score = 0usize;
                let mut sampled_txbs = 0usize;
                for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
                    for row in 0..chroma_span.height {
                        for col in 0..chroma_span.width {
                            if !lossy_mode_search_samples_txb(
                                row,
                                col,
                                chroma_span.width,
                                chroma_span.height,
                            ) {
                                continue;
                            }
                            let (x0, y0) = self.txb_origin(
                                plane,
                                chroma_span.col + col,
                                chroma_span.row + row,
                            );
                            paeth_score += self.paeth_txb_score_for_score(
                                plane,
                                x0,
                                y0,
                                chroma_context,
                                Av2CoefficientProxyKind::ChromaTransform,
                            );
                            sampled_txbs += 1;
                        }
                    }
                }
                if sampled_txbs == 0 || sampled_txbs == chroma_total_txbs {
                    paeth_score
                } else {
                    paeth_score.saturating_mul(chroma_total_txbs) / sampled_txbs
                }
            });
        let chroma_smooth_scores =
            lossy_chroma_smooth_search_allowed(chroma_scores, chroma_total_txbs).then(|| {
                let mut smooth_scores = Av2LossyIntraTxbScores::default();
                let mut sampled_txbs = 0usize;
                for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
                    for row in 0..chroma_span.height {
                        for col in 0..chroma_span.width {
                            if !lossy_mode_search_samples_txb(
                                row,
                                col,
                                chroma_span.width,
                                chroma_span.height,
                            ) {
                                continue;
                            }
                            let (x0, y0) = self.txb_origin(
                                plane,
                                chroma_span.col + col,
                                chroma_span.row + row,
                            );
                            smooth_scores.add_assign(self.smooth_txb_scores_for_score(
                                plane,
                                x0,
                                y0,
                                chroma_context,
                                Av2CoefficientProxyKind::ChromaTransform,
                            ));
                            sampled_txbs += 1;
                        }
                    }
                }
                smooth_scores.scaled_to_txb_count(chroma_total_txbs, sampled_txbs)
            });
        let mut best_chroma = (mode.chroma_use_bdpcm, mode.chroma_intra_mode, usize::MAX);
        for (chroma_use_bdpcm, chroma_intra_mode, base_score, syntax_penalty) in [
            (
                false,
                Av2ChromaIntraMode::Horizontal,
                chroma_scores.horizontal,
                lossy_chroma_mode_syntax_penalty(
                    mode.coded_luma_mode(),
                    Av2ChromaIntraMode::Horizontal,
                ),
            ),
            (
                false,
                Av2ChromaIntraMode::Vertical,
                chroma_scores.vertical,
                lossy_chroma_mode_syntax_penalty(
                    mode.coded_luma_mode(),
                    Av2ChromaIntraMode::Vertical,
                ),
            ),
            (
                false,
                Av2ChromaIntraMode::Dc,
                chroma_scores.dc,
                lossy_chroma_mode_syntax_penalty(mode.coded_luma_mode(), Av2ChromaIntraMode::Dc),
            ),
        ] {
            let score = base_score + syntax_penalty;
            if score < best_chroma.2 {
                best_chroma = (chroma_use_bdpcm, chroma_intra_mode, score);
            }
        }
        if let Some(paeth_score) = chroma_paeth_score {
            let score = paeth_score
                + lossy_chroma_mode_syntax_penalty(
                    mode.coded_luma_mode(),
                    Av2ChromaIntraMode::Paeth,
                );
            if score < best_chroma.2 {
                best_chroma = (false, Av2ChromaIntraMode::Paeth, score);
            }
        }
        if let Some(smooth_scores) = chroma_smooth_scores {
            for (chroma_intra_mode, score) in [
                (Av2ChromaIntraMode::Smooth, smooth_scores.smooth),
                (
                    Av2ChromaIntraMode::SmoothVertical,
                    smooth_scores.smooth_vertical,
                ),
                (
                    Av2ChromaIntraMode::SmoothHorizontal,
                    smooth_scores.smooth_horizontal,
                ),
            ] {
                let score = score
                    + lossy_chroma_mode_syntax_penalty(mode.coded_luma_mode(), chroma_intra_mode)
                    + 0usize;
                if score < best_chroma.2 {
                    best_chroma = (false, chroma_intra_mode, score);
                }
            }
        }
        mode.chroma_use_bdpcm = best_chroma.0;
        mode.chroma_intra_mode = best_chroma.1;
        if !mode.chroma_use_bdpcm
            && mode.chroma_intra_mode != Av2ChromaIntraMode::Dc
            && lossy_regular_q_dc_refinement_allowed(
                best_chroma.2,
                chroma_scores.dc
                    + lossy_chroma_mode_syntax_penalty(
                        mode.coded_luma_mode(),
                        Av2ChromaIntraMode::Dc,
                    ),
                chroma_total_txbs,
            )
        {
            let chroma_context = Av2LossyLeafPredictorContext {
                leaf_x0: chroma_leaf_x0,
                leaf_y0: chroma_leaf_y0,
                leaf_width: chroma_leaf_width,
                leaf_height: chroma_leaf_height,
                coded_mi_context,
            };
            let selected_score = self.sampled_chroma_regular_q_leaf_score(
                chroma_span,
                chroma_context,
                mode,
            );
            let dc_mode = Av2LossySubsampledModeDecision {
                chroma_use_bdpcm: false,
                chroma_intra_mode: Av2ChromaIntraMode::Dc,
                ..mode
            };
            let dc_score =
                self.sampled_chroma_regular_q_leaf_score(chroma_span, chroma_context, dc_mode);
            if lossy_regular_q_refinement_selects_dc(
                selected_score,
                dc_score,
                chroma_total_txbs,
                self.quant_step(),
            ) {
                mode.chroma_use_bdpcm = false;
                mode.chroma_intra_mode = Av2ChromaIntraMode::Dc;
            }
        }
        if self.chroma_format == Av2ChromaFormat::Yuv444
            && lossy_fsc_search_allowed(txb_width * txb_height + chroma_total_txbs)
        {
            let (fsc_score, transform_score) = self.fsc_leaf_scores(
                decision,
                txb_width,
                txb_height,
                luma_context,
                chroma_span,
                chroma_leaf_x0,
                chroma_leaf_y0,
                chroma_leaf_width,
                chroma_leaf_height,
                coded_mi_context,
                mode,
            );
            mode.use_fsc = fsc_score + 96 < transform_score;
        }
        self.record_leaf(
            decision.block_size,
            txb_width * txb_height,
            chroma_span.width * chroma_span.height * 2,
            mode,
        );
        mode
    }

    fn sampled_luma_regular_q_leaf_score(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        mode: Av2LossySubsampledModeDecision,
        luma_mode_syntax: Av2LumaModeSyntax,
    ) -> usize {
        let mut score =
            lossy_luma_refinement_syntax_penalty(mode.coded_luma_mode(), luma_mode_syntax);
        let mut sampled_txbs = 0usize;
        for row in 0..txb_height {
            for col in 0..txb_width {
                if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                    continue;
                }
                let (x0, y0) =
                    self.txb_origin(Av2LossyPlane::Y, decision.col + col, decision.row + row);
                let analysis = self.analyze_txb(Av2LossyPlane::Y, x0, y0, mode, context);
                score +=
                    self.regular_q_txb_rd_score(&analysis, Av2CoefficientProxyKind::LumaTransform);
                sampled_txbs += 1;
            }
        }
        lossy_scale_sampled_score(score, txb_width * txb_height, sampled_txbs)
    }

    fn sampled_chroma_regular_q_leaf_score(
        &self,
        chroma_span: Av2ChromaTx4x4Span,
        context: Av2LossyLeafPredictorContext<'_>,
        mode: Av2LossySubsampledModeDecision,
    ) -> usize {
        let mut score =
            lossy_chroma_mode_syntax_penalty(mode.coded_luma_mode(), mode.chroma_intra_mode);
        let mut sampled_txbs = 0usize;
        for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
            for row in 0..chroma_span.height {
                for col in 0..chroma_span.width {
                    if !lossy_mode_search_samples_txb(
                        row,
                        col,
                        chroma_span.width,
                        chroma_span.height,
                    ) {
                        continue;
                    }
                    let (x0, y0) =
                        self.txb_origin(plane, chroma_span.col + col, chroma_span.row + row);
                    let analysis = self.analyze_txb(plane, x0, y0, mode, context);
                    score += self
                        .regular_q_txb_rd_score(&analysis, Av2CoefficientProxyKind::ChromaTransform);
                    sampled_txbs += 1;
                }
            }
        }
        lossy_scale_sampled_score(
            score,
            chroma_span.width * chroma_span.height * 2,
            sampled_txbs,
        )
    }

    fn regular_q_txb_rd_score(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        kind: Av2CoefficientProxyKind,
    ) -> usize {
        let candidate = choose_regular_q_lossy_txb(
            self.regular_dct_quantized_residual_candidates(analysis),
            kind,
            self.quant_step(),
        );
        let rate = coefficient_proxy_score(&candidate.coefficients, kind);
        lossy_txb_score(
            rate,
            candidate.sse,
            candidate.variance_loss,
            regular_q_rd_quant_step(self.quant_step()),
        )
    }

    fn fsc_leaf_scores(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        luma_context: Av2LossyLeafPredictorContext<'_>,
        chroma_span: Av2ChromaTx4x4Span,
        chroma_leaf_x0: usize,
        chroma_leaf_y0: usize,
        chroma_leaf_width: usize,
        chroma_leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        mode: Av2LossySubsampledModeDecision,
    ) -> (usize, usize) {
        let mut fsc_score = 96usize;
        let mut transform_score = 0usize;
        for row in 0..txb_height {
            for col in 0..txb_width {
                if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                    continue;
                }
                let (x0, y0) =
                    self.txb_origin(Av2LossyPlane::Y, decision.col + col, decision.row + row);
                let analysis = self.analyze_txb(Av2LossyPlane::Y, x0, y0, mode, luma_context);
                fsc_score += coefficient_proxy_score(
                    &tx4x4_coefficients_from_residual(&analysis.residual, true),
                    Av2CoefficientProxyKind::LumaIdtx,
                );
                transform_score += coefficient_proxy_score(
                    &tx4x4_coefficients_from_residual(&analysis.residual, false),
                    Av2CoefficientProxyKind::LumaTransform,
                );
            }
        }

        let chroma_context = Av2LossyLeafPredictorContext {
            leaf_x0: chroma_leaf_x0,
            leaf_y0: chroma_leaf_y0,
            leaf_width: chroma_leaf_width,
            leaf_height: chroma_leaf_height,
            coded_mi_context,
        };
        for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
            for row in 0..chroma_span.height {
                for col in 0..chroma_span.width {
                    if !lossy_mode_search_samples_txb(
                        row,
                        col,
                        chroma_span.width,
                        chroma_span.height,
                    ) {
                        continue;
                    }
                    let (x0, y0) =
                        self.txb_origin(plane, chroma_span.col + col, chroma_span.row + row);
                    let analysis = self.analyze_txb(plane, x0, y0, mode, chroma_context);
                    fsc_score += coefficient_proxy_score(
                        &tx4x4_coefficients_from_residual(&analysis.residual, true),
                        Av2CoefficientProxyKind::ChromaTransform,
                    );
                    transform_score += coefficient_proxy_score(
                        &tx4x4_coefficients_from_residual(&analysis.residual, false),
                        Av2CoefficientProxyKind::ChromaTransform,
                    );
                }
            }
        }

        (fsc_score, transform_score)
    }

    fn intra_txb_scores_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
        score_paeth: bool,
    ) -> Av2LossyIntraTxbScores {
        let mut source = [0; TX4X4_SAMPLES];
        let dc = i32::from(self.dc_predictor_for_score(plane, x0, y0, context));
        let mut h_pred = [0; TX4X4_SIZE];
        let mut v_pred = [0; TX4X4_SIZE];
        for index in 0..TX4X4_SIZE {
            h_pred[index] = self.h_predictor_for_score(plane, x0, y0, index, context);
            v_pred[index] = self.v_predictor_for_score(plane, x0, y0, index, context);
        }
        let above_left = if score_paeth {
            self.above_left_predictor_for_score(plane, x0, y0, context)
        } else {
            0
        };
        let mut scores = Av2LossyIntraTxbScores {
            dc: 16,
            horizontal: 16,
            vertical: 16,
            paeth: 16,
            smooth: 0,
            smooth_vertical: 0,
            smooth_horizontal: 0,
        };
        let mut dc_sum = 0i32;
        let mut horizontal_sum = 0i32;
        let mut vertical_sum = 0i32;
        let mut paeth_sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                source[index] = sample as Av2Sample;
                let horizontal = i32::from(h_pred[local_y]);
                let vertical = i32::from(v_pred[local_x]);
                let dc_diff = sample - dc;
                let horizontal_diff = sample - horizontal;
                let vertical_diff = sample - vertical;
                dc_sum += dc_diff;
                horizontal_sum += horizontal_diff;
                vertical_sum += vertical_diff;
                add_residual_sample_proxy_score(&mut scores.dc, dc_diff, magnitude_scale);
                add_residual_sample_proxy_score(
                    &mut scores.horizontal,
                    horizontal_diff,
                    magnitude_scale,
                );
                add_residual_sample_proxy_score(
                    &mut scores.vertical,
                    vertical_diff,
                    magnitude_scale,
                );
                if score_paeth {
                    let paeth =
                        i32::from(paeth_predictor(h_pred[local_y], v_pred[local_x], above_left));
                    let paeth_diff = sample - paeth;
                    paeth_sum += paeth_diff;
                    add_residual_sample_proxy_score(&mut scores.paeth, paeth_diff, magnitude_scale);
                }
            }
        }
        let max_delta = i32::from(self.bit_depth.max_sample());
        let dc_delta = quantize_i32_to_step(
            round_div_i32(dc_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let horizontal_delta = quantize_i32_to_step(
            round_div_i32(horizontal_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let vertical_delta = quantize_i32_to_step(
            round_div_i32(vertical_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let paeth_delta = if score_paeth {
            quantize_i32_to_step(
                round_div_i32(paeth_sum, TX4X4_SAMPLES as i32),
                lossy_dc_delta_quant_step(self.quant_step()),
            )
            .clamp(-max_delta, max_delta)
        } else {
            0
        };
        let mut dc_sse = 0usize;
        let mut horizontal_sse = 0usize;
        let mut vertical_sse = 0usize;
        let mut paeth_sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let source = i32::from(source[index]);
                let dc_recon = (dc + dc_delta).clamp(0, max_sample);
                let horizontal_recon =
                    (i32::from(h_pred[local_y]) + horizontal_delta).clamp(0, max_sample);
                let vertical_recon =
                    (i32::from(v_pred[local_x]) + vertical_delta).clamp(0, max_sample);
                let dc_diff = source - dc_recon;
                let horizontal_diff = source - horizontal_recon;
                let vertical_diff = source - vertical_recon;
                dc_sse += (dc_diff * dc_diff) as usize;
                horizontal_sse += (horizontal_diff * horizontal_diff) as usize;
                vertical_sse += (vertical_diff * vertical_diff) as usize;
                if score_paeth {
                    let paeth =
                        i32::from(paeth_predictor(h_pred[local_y], v_pred[local_x], above_left));
                    let paeth_recon = (paeth + paeth_delta).clamp(0, max_sample);
                    let paeth_diff = source - paeth_recon;
                    paeth_sse += (paeth_diff * paeth_diff) as usize;
                }
            }
        }

        scores.dc = lossy_txb_score(scores.dc, dc_sse, 0, self.quant_step());
        scores.horizontal = lossy_txb_score(scores.horizontal, horizontal_sse, 0, self.quant_step());
        scores.vertical = lossy_txb_score(scores.vertical, vertical_sse, 0, self.quant_step());
        if score_paeth {
            scores.paeth = lossy_txb_score(scores.paeth, paeth_sse, 0, self.quant_step());
        }
        scores
    }

    fn smooth_txb_scores_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
    ) -> Av2LossyIntraTxbScores {
        let (smooth_above, smooth_left) = self.smooth_edges_for_score(plane, x0, y0, context);
        let mut source = [0; TX4X4_SAMPLES];
        let mut smooth_pred = [0; TX4X4_SAMPLES];
        let mut smooth_vertical_pred = [0; TX4X4_SAMPLES];
        let mut smooth_horizontal_pred = [0; TX4X4_SAMPLES];
        let mut scores = Av2LossyIntraTxbScores {
            dc: 0,
            horizontal: 0,
            vertical: 0,
            paeth: 0,
            smooth: 16,
            smooth_vertical: 16,
            smooth_horizontal: 16,
        };
        let mut smooth_sum = 0i32;
        let mut smooth_vertical_sum = 0i32;
        let mut smooth_horizontal_sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                source[index] = sample as Av2Sample;
                let (smooth, smooth_v, smooth_h) = av2_highbd_smooth_intra_predictor_set(
                    smooth_above,
                    smooth_left,
                    local_x,
                    local_y,
                    self.bit_depth,
                );
                smooth_pred[index] = smooth;
                smooth_vertical_pred[index] = smooth_v;
                smooth_horizontal_pred[index] = smooth_h;
                let smooth_diff = sample - i32::from(smooth);
                let smooth_vertical_diff = sample - i32::from(smooth_v);
                let smooth_horizontal_diff = sample - i32::from(smooth_h);
                smooth_sum += smooth_diff;
                smooth_vertical_sum += smooth_vertical_diff;
                smooth_horizontal_sum += smooth_horizontal_diff;
                add_residual_sample_proxy_score(&mut scores.smooth, smooth_diff, magnitude_scale);
                add_residual_sample_proxy_score(
                    &mut scores.smooth_vertical,
                    smooth_vertical_diff,
                    magnitude_scale,
                );
                add_residual_sample_proxy_score(
                    &mut scores.smooth_horizontal,
                    smooth_horizontal_diff,
                    magnitude_scale,
                );
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let smooth_delta = quantize_i32_to_step(
            round_div_i32(smooth_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let smooth_vertical_delta = quantize_i32_to_step(
            round_div_i32(smooth_vertical_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let smooth_horizontal_delta = quantize_i32_to_step(
            round_div_i32(smooth_horizontal_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut smooth_sse = 0usize;
        let mut smooth_vertical_sse = 0usize;
        let mut smooth_horizontal_sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let source = i32::from(source[index]);
            let smooth_recon = (i32::from(smooth_pred[index]) + smooth_delta).clamp(0, max_sample);
            let smooth_vertical_recon =
                (i32::from(smooth_vertical_pred[index]) + smooth_vertical_delta)
                    .clamp(0, max_sample);
            let smooth_horizontal_recon =
                (i32::from(smooth_horizontal_pred[index]) + smooth_horizontal_delta)
                    .clamp(0, max_sample);
            let smooth_diff = source - smooth_recon;
            let smooth_vertical_diff = source - smooth_vertical_recon;
            let smooth_horizontal_diff = source - smooth_horizontal_recon;
            smooth_sse += (smooth_diff * smooth_diff) as usize;
            smooth_vertical_sse += (smooth_vertical_diff * smooth_vertical_diff) as usize;
            smooth_horizontal_sse += (smooth_horizontal_diff * smooth_horizontal_diff) as usize;
        }

        scores.smooth = lossy_txb_score(scores.smooth, smooth_sse, 0, self.quant_step());
        scores.smooth_vertical =
            lossy_txb_score(scores.smooth_vertical, smooth_vertical_sse, 0, self.quant_step());
        scores.smooth_horizontal = lossy_txb_score(
            scores.smooth_horizontal,
            smooth_horizontal_sse,
            0,
            self.quant_step(),
        );
        scores
    }

    fn paeth_txb_score_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
    ) -> usize {
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let mut score = 16usize;
        let mut sum = 0i32;
        let mut h_pred = [0; TX4X4_SIZE];
        let mut v_pred = [0; TX4X4_SIZE];
        for index in 0..TX4X4_SIZE {
            h_pred[index] = self.h_predictor_for_score(plane, x0, y0, index, context);
            v_pred[index] = self.v_predictor_for_score(plane, x0, y0, index, context);
        }
        let above_left = self.above_left_predictor_for_score(plane, x0, y0, context);
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                let paeth = i32::from(paeth_predictor(
                    h_pred[local_y],
                    v_pred[local_x],
                    above_left,
                ));
                let diff = sample - paeth;
                source[index] = sample as Av2Sample;
                predictor[index] = paeth as Av2Sample;
                sum += diff;
                add_residual_sample_proxy_score(&mut score, diff, magnitude_scale);
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(
            round_div_i32(sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let recon = (i32::from(predictor[index]) + delta).clamp(0, max_sample);
            let diff = i32::from(source[index]) - recon;
            sse += (diff * diff) as usize;
        }

        lossy_txb_score(score, sse, 0, self.quant_step())
    }

    fn directional_txb_score_for_score(
        &self,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
        luma_intra_mode: Av2LumaIntraMode,
    ) -> usize {
        let angle = lossy_luma_idif_angle(luma_intra_mode)
            .expect("directional score is only requested for non-cardinal luma IDIF modes");
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, context);
        let (constant, edges) = self.luma_directional_idif_predictor_state_with(
            Av2LossyPlane::Y,
            x0,
            y0,
            angle,
            context,
            &edge_sample,
        );
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let mut score = 16usize;
        let mut sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(Av2LossyPlane::Y, x0 + local_x, y0 + local_y));
                let pred = constant.unwrap_or_else(|| {
                    luma_directional_idif_predictor(
                        angle,
                        edges.expect("IDIF edges are precomputed"),
                        local_x,
                        local_y,
                        self.bit_depth,
                    )
                });
                let diff = sample - i32::from(pred);
                source[index] = sample as Av2Sample;
                predictor[index] = pred;
                sum += diff;
                add_residual_sample_proxy_score(&mut score, diff, magnitude_scale);
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(
            round_div_i32(sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let recon = (i32::from(predictor[index]) + delta).clamp(0, max_sample);
            let diff = i32::from(source[index]) - recon;
            sse += (diff * diff) as usize;
        }

        lossy_txb_score(score, sse, 0, self.quant_step())
    }

    fn luma_directional_idif_predictor_state_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        angle: i16,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> (Option<Av2Sample>, Option<DirectionalIdifEdges>)
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let base = av2_lossless_dc_predictor(self.bit_depth);

        let constant_predictor = match angle {
            1..=89 if !have_top => Some(if have_left {
                edge_sample(plane, x0 - 1, y0)
            } else {
                base.saturating_sub(1)
            }),
            181..=269 if !have_left => Some(if have_top {
                edge_sample(plane, x0, y0 - 1)
            } else {
                base.saturating_add(1)
            }),
            _ => None,
        };

        let edges = constant_predictor
            .is_none()
            .then(|| self.luma_directional_idif_edges_with(plane, x0, y0, angle, context, edge_sample));

        (constant_predictor, edges)
    }

    fn luma_directional_idif_edges_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        angle: i16,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> DirectionalIdifEdges
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let above_core = self.directional_above_edge_with(plane, x0, y0, context, edge_sample);
        let left_core = self.directional_left_edge_with(plane, x0, y0, context, edge_sample);
        let above_left = self.above_left_predictor_with(plane, x0, y0, edge_sample);
        let mut edges = DirectionalIdifEdges::new(self.bit_depth);
        edges.set_above(-2, above_left);
        edges.set_above(-1, above_left);
        edges.set_left(-2, above_left);
        edges.set_left(-1, above_left);
        for index in 0..8 {
            edges.set_above(index as i32, above_core[index]);
            edges.set_left(index as i32, left_core[index]);
        }
        if angle > 90 && angle < 180 {
            for index in TX4X4_SIZE..8 {
                edges.set_above(index as i32, above_core[TX4X4_SIZE - 1]);
                edges.set_left(index as i32, left_core[TX4X4_SIZE - 1]);
            }
        }
        edges.set_above(8, edges.above(7));
        edges.set_above(9, edges.above(7));
        edges.set_left(8, edges.left(7));
        edges.set_left(9, edges.left(7));
        edges
    }

    fn above_left_predictor_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        if have_left && have_top {
            edge_sample(plane, x0 - 1, y0 - 1)
        } else if have_top {
            edge_sample(plane, x0, y0 - 1)
        } else if have_left {
            edge_sample(plane, x0 - 1, y0)
        } else {
            av2_lossless_dc_predictor(self.bit_depth)
        }
    }

    fn directional_above_edge_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (plane_width, _) = self.plane_geometry(plane);
        let (plane_region_right, _) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut above = [av2_lossless_v_pred_above_edge(self.bit_depth); 8];
        if have_top {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
            let sb_right = (sb_origin_x + plane_sb_width)
                .min(plane_width)
                .min(plane_region_right);
            let superblock_top_row = y0 % plane_sb_height == 0;
            for index in 0..above.len() {
                let x = x0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_top_right_coded =
                    overhang && y0 == context.leaf_y0 && x < plane_region_right && {
                        let (row_mi, col_mi) =
                            self.coded_mi_for_plane_sample(plane, x, y0 - 1);
                        superblock_top_row
                            || (x < sb_right && context.coded_mi_context.is_coded(row_mi, col_mi))
                    };
                if x < plane_region_right
                    && (!overhang
                        || x < context.leaf_x0 + context.leaf_width
                        || external_top_right_coded)
                {
                    above[index] = edge_sample(plane, x, y0 - 1);
                } else if index > 0 {
                    above[index] = above[index - 1];
                }
            }
        } else if have_left {
            above.fill(edge_sample(plane, x0 - 1, y0));
        }
        above
    }

    fn directional_left_edge_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (_, plane_height) = self.plane_geometry(plane);
        let (_, plane_region_bottom) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut left = [av2_lossless_h_pred_left_edge(self.bit_depth); 8];
        if have_left {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
            let sb_bottom = (sb_origin_y + plane_sb_height)
                .min(plane_height)
                .min(plane_region_bottom);
            let superblock_left_col = x0 % plane_sb_width == 0;
            for index in 0..left.len() {
                let y = y0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_bottom_left_coded =
                    overhang && x0 == context.leaf_x0 && y < sb_bottom && {
                        let (row_mi, col_mi) =
                            self.coded_mi_for_plane_sample(plane, x0 - 1, y);
                        superblock_left_col || context.coded_mi_context.is_coded(row_mi, col_mi)
                    };
                if y < plane_region_bottom
                    && (!overhang
                        || (x0 == context.leaf_x0
                            && (y < context.leaf_y0 + context.leaf_height
                                || external_bottom_left_coded)))
                {
                    left[index] = edge_sample(plane, x0 - 1, y);
                } else if index > 0 {
                    left[index] = left[index - 1];
                }
            }
        } else if have_top {
            left.fill(edge_sample(plane, x0, y0 - 1));
        }
        left
    }

    fn dc_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        if !have_left && !have_top {
            return av2_lossless_dc_predictor(self.bit_depth);
        }

        let mut sum = 0u32;
        let mut count = 0u32;
        if have_top {
            for x in x0..(x0 + TX4X4_SIZE) {
                sum += u32::from(self.neighbor_sample_for_score(plane, x, y0 - 1, context));
                count += 1;
            }
        }
        if have_left {
            for y in y0..(y0 + TX4X4_SIZE) {
                sum += u32::from(self.neighbor_sample_for_score(plane, x0 - 1, y, context));
                count += 1;
            }
        }
        av2_reference_dc_average(sum, count)
    }

    fn h_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        if x0 > tile_origin_x {
            self.neighbor_sample_for_score(plane, x0 - 1, y0 + local_y, context)
        } else if y0 > tile_origin_y {
            self.neighbor_sample_for_score(plane, x0, y0 - 1, context)
        } else {
            av2_lossless_h_pred_left_edge(self.bit_depth)
        }
    }

    fn v_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        if y0 > tile_origin_y {
            self.neighbor_sample_for_score(plane, x0 + local_x, y0 - 1, context)
        } else if x0 > tile_origin_x {
            self.neighbor_sample_for_score(plane, x0 - 1, y0, context)
        } else {
            av2_lossless_v_pred_above_edge(self.bit_depth)
        }
    }

    fn above_left_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        if have_left && have_top {
            self.neighbor_sample_for_score(plane, x0 - 1, y0 - 1, context)
        } else if have_top {
            self.neighbor_sample_for_score(plane, x0, y0 - 1, context)
        } else if have_left {
            self.neighbor_sample_for_score(plane, x0 - 1, y0, context)
        } else {
            av2_lossless_dc_predictor(self.bit_depth)
        }
    }

    fn smooth_edges(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.smooth_edges_with(plane, x0, y0, context, &edge_sample)
    }

    fn smooth_edges_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample = |plane, x, y| self.neighbor_sample_for_score(plane, x, y, context);
        self.smooth_edges_with(plane, x0, y0, context, &edge_sample)
    }

    fn smooth_edges_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1])
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let (plane_region_right, plane_region_bottom) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut above = [av2_lossless_v_pred_above_edge(self.bit_depth); TX4X4_SIZE + 1];
        let mut left = [av2_lossless_h_pred_left_edge(self.bit_depth); TX4X4_SIZE + 1];

        if have_top {
            for local_x in 0..TX4X4_SIZE {
                above[local_x] = edge_sample(plane, x0 + local_x, y0 - 1);
            }
        } else if have_left {
            above[..TX4X4_SIZE].fill(edge_sample(plane, x0 - 1, y0));
        }

        if have_left {
            for local_y in 0..TX4X4_SIZE {
                left[local_y] = edge_sample(plane, x0 - 1, y0 + local_y);
            }
        } else if have_top {
            left[..TX4X4_SIZE].fill(edge_sample(plane, x0, y0 - 1));
        }

        let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
        let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
        let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
        let sb_right = (sb_origin_x + plane_sb_width)
            .min(plane_width)
            .min(plane_region_right);
        let top_right_x = x0 + TX4X4_SIZE;
        let superblock_top_row = y0 % plane_sb_height == 0;
        let external_top_right_coded =
            have_top && y0 == context.leaf_y0 && top_right_x < plane_region_right && {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, top_right_x, y0 - 1);
                superblock_top_row
                    || (top_right_x < sb_right && context.coded_mi_context.is_coded(row_mi, col_mi))
            };
        if have_top
            && top_right_x < plane_region_right
            && (top_right_x < context.leaf_x0 + context.leaf_width || external_top_right_coded)
        {
            above[TX4X4_SIZE] = edge_sample(plane, top_right_x, y0 - 1);
        } else {
            above[TX4X4_SIZE] = above[TX4X4_SIZE - 1];
        }

        let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
        let sb_bottom = (sb_origin_y + plane_sb_height)
            .min(plane_height)
            .min(plane_region_bottom);
        let bottom_left_y = y0 + TX4X4_SIZE;
        let superblock_left_col = x0 % plane_sb_width == 0;
        let external_bottom_left_coded =
            have_left && x0 == context.leaf_x0 && bottom_left_y < sb_bottom && {
                let (row_mi, col_mi) =
                    self.coded_mi_for_plane_sample(plane, x0 - 1, bottom_left_y);
                superblock_left_col || context.coded_mi_context.is_coded(row_mi, col_mi)
            };
        if have_left
            && x0 == context.leaf_x0
            && bottom_left_y < plane_region_bottom
            && (bottom_left_y < context.leaf_y0 + context.leaf_height
                || external_bottom_left_coded)
        {
            left[TX4X4_SIZE] = edge_sample(plane, x0 - 1, bottom_left_y);
        } else {
            left[TX4X4_SIZE] = left[TX4X4_SIZE - 1];
        }

        (above, left)
    }

    fn neighbor_sample_for_score(
        &self,
        plane: Av2LossyPlane,
        x: usize,
        y: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        if x >= context.leaf_x0
            && x < context.leaf_x0 + context.leaf_width
            && y >= context.leaf_y0
            && y < context.leaf_y0 + context.leaf_height
        {
            self.source_sample(plane, x, y)
        } else {
            self.recon_sample(plane, x, y)
        }
    }

    fn quant_step(&self) -> i32 {
        i32::from(self.base_qindex) << u32::from(self.bit_depth.bits() - 8)
    }

    fn base_qindex(&self) -> u16 {
        self.base_qindex
    }

    fn record_leaf(
        &self,
        block_size: Av2MvpBlockSize,
        luma_txbs: usize,
        chroma_txbs: usize,
        mode: Av2LossySubsampledModeDecision,
    ) {
        #[cfg(feature = "av2-lossy-stats")]
        if let Some(stats) = &self.stats {
            stats
                .borrow_mut()
                .record_leaf(block_size, luma_txbs, chroma_txbs, mode);
        }
        #[cfg(not(feature = "av2-lossy-stats"))]
        let _ = (block_size, luma_txbs, chroma_txbs, mode);
    }

    fn record_txb_choice(
        &self,
        plane: Av2LossyPlane,
        choice: &Av2LossyTxbChoice,
        analysis: &Av2LossyTxbAnalysis,
    ) {
        #[cfg(feature = "av2-lossy-stats")]
        if let Some(stats) = &self.stats {
            stats
                .borrow_mut()
                .record_txb_choice(plane, choice, analysis);
        }
        #[cfg(not(feature = "av2-lossy-stats"))]
        let _ = (plane, choice, analysis);
    }
}
