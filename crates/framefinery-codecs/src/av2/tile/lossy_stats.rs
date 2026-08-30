#[cfg(feature = "av2-lossy-stats")]
#[derive(Debug, Default)]
struct Av2LossyStats {
    leaves: u64,
    leaf_txbs_luma: u64,
    leaf_txbs_chroma: u64,
    leaf_blocks_8: u64,
    leaf_blocks_16: u64,
    leaf_blocks_32: u64,
    leaf_blocks_64: u64,
    leaf_blocks_other: u64,
    luma_modes: Av2LossyModeStats,
    chroma_modes: Av2LossyChromaModeStats,
    y: Av2LossyPlaneStats,
    u: Av2LossyPlaneStats,
    v: Av2LossyPlaneStats,
}

#[cfg(feature = "av2-lossy-stats")]
impl Av2LossyStats {
    fn record_leaf(
        &mut self,
        block_size: Av2MvpBlockSize,
        luma_txbs: usize,
        chroma_txbs: usize,
        mode: Av2LossySubsampledModeDecision,
    ) {
        self.leaves += 1;
        self.leaf_txbs_luma += luma_txbs as u64;
        self.leaf_txbs_chroma += chroma_txbs as u64;
        match block_size.width.max(block_size.height) {
            0..=8 => self.leaf_blocks_8 += 1,
            9..=16 => self.leaf_blocks_16 += 1,
            17..=32 => self.leaf_blocks_32 += 1,
            33..=64 => self.leaf_blocks_64 += 1,
            _ => self.leaf_blocks_other += 1,
        }
        self.luma_modes.record(mode.luma_intra_mode);
        self.chroma_modes.record(mode.chroma_intra_mode);
        if mode.use_fsc {
            self.luma_modes.fsc += 1;
        }
    }

    fn record_txb_choice(
        &mut self,
        plane: Av2LossyPlane,
        choice: &Av2LossyTxbChoice,
        analysis: &Av2LossyTxbAnalysis,
    ) {
        let stats = match plane {
            Av2LossyPlane::Y => &mut self.y,
            Av2LossyPlane::U => &mut self.u,
            Av2LossyPlane::V => &mut self.v,
        };
        stats.record(choice, analysis);
    }

    fn print(
        &self,
        region: Av2TileRegion,
        chroma_format: Av2ChromaFormat,
        bit_depth: SampleBitDepth,
        qp: u8,
    ) {
        eprintln!(
            "av2-lossy-stats region={}x{}+{},{} chroma={:?} bit_depth={} qp={} leaves={} leaf_txbs_luma={} leaf_txbs_chroma={} leaf_blocks_8={} leaf_blocks_16={} leaf_blocks_32={} leaf_blocks_64={} leaf_blocks_other={}",
            region.width,
            region.height,
            region.origin_x,
            region.origin_y,
            chroma_format,
            bit_depth.bits(),
            qp,
            self.leaves,
            self.leaf_txbs_luma,
            self.leaf_txbs_chroma,
            self.leaf_blocks_8,
            self.leaf_blocks_16,
            self.leaf_blocks_32,
            self.leaf_blocks_64,
            self.leaf_blocks_other,
        );
        self.luma_modes.print("luma_modes");
        self.chroma_modes.print("chroma_modes");
        self.y.print("plane_y");
        self.u.print("plane_u");
        self.v.print("plane_v");
    }
}

#[cfg(feature = "av2-lossy-stats")]
#[derive(Debug, Default)]
struct Av2LossyModeStats {
    dc: u64,
    horizontal: u64,
    vertical: u64,
    directional: u64,
    paeth: u64,
    smooth: u64,
    fsc: u64,
    other: u64,
}

#[cfg(feature = "av2-lossy-stats")]
impl Av2LossyModeStats {
    fn record(&mut self, mode: Av2LumaIntraMode) {
        match mode {
            Av2LumaIntraMode::Dc => self.dc += 1,
            Av2LumaIntraMode::Horizontal => self.horizontal += 1,
            Av2LumaIntraMode::Vertical => self.vertical += 1,
            mode if lossy_luma_idif_angle(mode).is_some() => self.directional += 1,
            Av2LumaIntraMode::Paeth => self.paeth += 1,
            Av2LumaIntraMode::Smooth
            | Av2LumaIntraMode::SmoothVertical
            | Av2LumaIntraMode::SmoothHorizontal => self.smooth += 1,
            _ => self.other += 1,
        }
    }

    fn print(&self, label: &str) {
        eprintln!(
            "av2-lossy-stats {label} dc={} horizontal={} vertical={} directional={} paeth={} smooth={} fsc={} other={}",
            self.dc, self.horizontal, self.vertical, self.directional, self.paeth, self.smooth, self.fsc, self.other
        );
    }
}

#[cfg(feature = "av2-lossy-stats")]
#[derive(Debug, Default)]
struct Av2LossyChromaModeStats {
    dc: u64,
    horizontal: u64,
    vertical: u64,
    paeth: u64,
    smooth: u64,
    other: u64,
}

#[cfg(feature = "av2-lossy-stats")]
impl Av2LossyChromaModeStats {
    fn record(&mut self, mode: Av2ChromaIntraMode) {
        match mode {
            Av2ChromaIntraMode::Dc => self.dc += 1,
            Av2ChromaIntraMode::Horizontal => self.horizontal += 1,
            Av2ChromaIntraMode::Vertical => self.vertical += 1,
            Av2ChromaIntraMode::Paeth => self.paeth += 1,
            Av2ChromaIntraMode::Smooth
            | Av2ChromaIntraMode::SmoothVertical
            | Av2ChromaIntraMode::SmoothHorizontal => self.smooth += 1,
            _ => self.other += 1,
        }
    }

    fn print(&self, label: &str) {
        eprintln!(
            "av2-lossy-stats {label} dc={} horizontal={} vertical={} paeth={} smooth={} other={}",
            self.dc, self.horizontal, self.vertical, self.paeth, self.smooth, self.other
        );
    }
}

#[cfg(feature = "av2-lossy-stats")]
#[derive(Debug, Default)]
struct Av2LossyPlaneStats {
    txbs: u64,
    exact: u64,
    exact_zero: u64,
    exact_nonzero: u64,
    dc_delta: u64,
    spatial: u64,
    refined_spatial: u64,
    transform: u64,
    regular_dct: u64,
    regular_dct_tail_pruned: u64,
    regular_dct_double_tail_pruned: u64,
    regular_dct_dc_only: u64,
    quantized_zero: u64,
    quantized_nonzero: u64,
    eob_1: u64,
    eob_2_4: u64,
    eob_5_8: u64,
    eob_9_16: u64,
    chosen_sse: u128,
    source_variance: u128,
    variance_loss: u128,
}

#[cfg(feature = "av2-lossy-stats")]
impl Av2LossyPlaneStats {
    fn record(&mut self, choice: &Av2LossyTxbChoice, analysis: &Av2LossyTxbAnalysis) {
        self.txbs += 1;
        self.source_variance += analysis.source_variance as u128;
        match choice {
            Av2LossyTxbChoice::Exact => {
                self.exact += 1;
                if tx4x4_residual_is_zero(&analysis.residual) {
                    self.exact_zero += 1;
                } else {
                    self.exact_nonzero += 1;
                }
            }
            Av2LossyTxbChoice::DcDelta(_) => {
                self.dc_delta += 1;
                self.chosen_sse += analysis.dc_sse as u128;
                self.variance_loss += analysis.dc_variance_loss as u128;
            }
            Av2LossyTxbChoice::QuantizedResidual(candidate) => {
                let eob = quantized_txb_eob(&candidate.coefficients);
                if eob == 0 {
                    self.quantized_zero += 1;
                } else {
                    self.quantized_nonzero += 1;
                }
                match eob {
                    0 => {}
                    1 => self.eob_1 += 1,
                    2..=4 => self.eob_2_4 += 1,
                    5..=8 => self.eob_5_8 += 1,
                    _ => self.eob_9_16 += 1,
                }
                match candidate.kind {
                    Av2LossyResidualCandidateKind::Spatial => self.spatial += 1,
                    Av2LossyResidualCandidateKind::RefinedSpatial => {
                        self.refined_spatial += 1;
                    }
                    Av2LossyResidualCandidateKind::Transform => self.transform += 1,
                    Av2LossyResidualCandidateKind::RegularDct => self.regular_dct += 1,
                    Av2LossyResidualCandidateKind::RegularDctTailPruned => {
                        self.regular_dct_tail_pruned += 1;
                    }
                    Av2LossyResidualCandidateKind::RegularDctDoubleTailPruned => {
                        self.regular_dct_double_tail_pruned += 1;
                    }
                    Av2LossyResidualCandidateKind::RegularDctDcOnly => {
                        self.regular_dct_dc_only += 1;
                    }
                }
                self.chosen_sse += candidate.sse as u128;
                self.variance_loss += candidate.variance_loss as u128;
            }
        }
    }

    fn print(&self, label: &str) {
        let txbs = u128::from(self.txbs.max(1));
        eprintln!(
            "av2-lossy-stats {label} txbs={} exact={} exact_zero={} exact_nonzero={} dc_delta={} spatial={} refined_spatial={} transform={} regular_dct={} regular_dct_tail_pruned={} regular_dct_double_tail_pruned={} regular_dct_dc_only={} quantized_zero={} quantized_nonzero={} eob_1={} eob_2_4={} eob_5_8={} eob_9_16={} avg_sse={} avg_source_variance={} avg_variance_loss={}",
            self.txbs,
            self.exact,
            self.exact_zero,
            self.exact_nonzero,
            self.dc_delta,
            self.spatial,
            self.refined_spatial,
            self.transform,
            self.regular_dct,
            self.regular_dct_tail_pruned,
            self.regular_dct_double_tail_pruned,
            self.regular_dct_dc_only,
            self.quantized_zero,
            self.quantized_nonzero,
            self.eob_1,
            self.eob_2_4,
            self.eob_5_8,
            self.eob_9_16,
            self.chosen_sse / txbs,
            self.source_variance / txbs,
            self.variance_loss / txbs,
        );
    }
}
