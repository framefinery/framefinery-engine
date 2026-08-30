#[cfg(feature = "vvc-stats")]
struct VvcStatsSink {
    sink: Option<JsonlInstrumentationSink>,
}

#[cfg(feature = "vvc-stats")]
const VVC_STATS_ENV: &str = "FRAMEFINERY_VVC_STATS";
#[cfg(feature = "vvc-stats")]
const VVC_CTU_BITS_ENV: &str = "FRAMEFINERY_VVC_CTU_BITS";

#[cfg(feature = "vvc-stats")]
impl VvcStatsSink {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            sink: JsonlInstrumentationSink::append_from_env(VVC_STATS_ENV)
                .map_err(|err| err.to_string())?,
        })
    }

    fn write_frame(&mut self, frame: &VvcFrameStats) -> Result<(), String> {
        let Some(sink) = self.sink.as_mut() else {
            return Ok(());
        };
        sink.write_json_line(&frame.to_json_line())
            .map_err(|err| err.to_string())?;
        sink.flush().map_err(|err| err.to_string())
    }
}

#[cfg(feature = "vvc-stats")]
struct VvcFrameStats {
    frame_idx: usize,
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    lossless: bool,
    slice_qp: i32,
    chroma_qp: i32,
    ctu_count: usize,
    bitstream_bytes: usize,
    stages: Vec<VvcStageStats>,
    counters: Vec<VvcCounterStats>,
}

#[cfg(feature = "vvc-stats")]
impl VvcFrameStats {
    fn new(
        frame_idx: usize,
        geometry: VvcVideoGeometry,
        format: VvcPictureFormat,
        lossless: bool,
        slice_qp: i32,
        chroma_qp: i32,
    ) -> Self {
        Self {
            frame_idx,
            width: geometry.width,
            height: geometry.height,
            chroma_sampling: format.chroma_sampling,
            bit_depth: format.bit_depth,
            lossless,
            slice_qp,
            chroma_qp,
            ctu_count: vvc_picture_ctu_count(geometry),
            bitstream_bytes: 0,
            stages: Vec::new(),
            counters: Vec::new(),
        }
    }

    fn add_elapsed(&mut self, name: &'static str, start: StageStart) {
        self.add_stage(name, start.elapsed_nanos(), 1);
    }

    fn add_stage(&mut self, name: &'static str, nanos: u64, count: u64) {
        if let Some(stage) = self.stages.iter_mut().find(|stage| stage.name == name) {
            stage.nanos += nanos;
            stage.count += count;
        } else {
            self.stages.push(VvcStageStats { name, nanos, count });
        }
    }

    fn set_bitstream_bytes(&mut self, bitstream_bytes: usize) {
        self.bitstream_bytes = bitstream_bytes;
    }

    fn add_counter(&mut self, name: &'static str, value: u64) {
        self.add_counter_named(name, value);
    }

    fn add_counter_named(&mut self, name: &str, value: u64) {
        if let Some(counter) = self
            .counters
            .iter_mut()
            .find(|counter| counter.name == name)
        {
            counter.value += value;
        } else {
            self.counters.push(VvcCounterStats {
                name: name.to_owned(),
                value,
            });
        }
    }

    fn to_json_line(&self) -> String {
        let mut json = format!(
            "{{\"kind\":\"framefinery.vvc.stats.v1\",\"frame_index\":{},\"width\":{},\"height\":{},\"chroma_sampling\":\"{:?}\",\"bit_depth\":{},\"lossless\":{},\"slice_qp\":{},\"chroma_qp\":{},\"ctu_count\":{},\"bitstream_bytes\":{},\"stages\":[",
            self.frame_idx,
            self.width,
            self.height,
            self.chroma_sampling,
            self.bit_depth.bits(),
            self.lossless,
            self.slice_qp,
            self.chroma_qp,
            self.ctu_count,
            self.bitstream_bytes
        );
        for (index, stage) in self.stages.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\":\"{}\",\"ns\":{},\"count\":{}}}",
                stage.name, stage.nanos, stage.count
            ));
        }
        json.push_str("],\"counters\":[");
        for (index, counter) in self.counters.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\":\"{}\",\"value\":{}}}",
                counter.name, counter.value
            ));
        }
        json.push_str("]}");
        json
    }
}

#[cfg(feature = "vvc-stats")]
struct VvcStageStats {
    name: &'static str,
    nanos: u64,
    count: u64,
}

#[cfg(feature = "vvc-stats")]
struct VvcCounterStats {
    name: String,
    value: u64,
}

#[cfg(feature = "vvc-stats")]
struct VvcCtuBitSink {
    sink: Option<JsonlInstrumentationSink>,
    frame_idx: Option<usize>,
    frame_state: Option<VvcFrameCtuCabacState>,
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct VvcCtuBitCategories {
    partition_bits: usize,
    luma_mode_bits: usize,
    chroma_mode_bits: usize,
    residual_bits: usize,
    intrabc_bits: usize,
    inter_bits: usize,
    palette_bits: usize,
    other_bits: usize,
}

#[cfg(feature = "vvc-stats")]
impl VvcCtuBitCategories {
    fn from_symbols(symbols: &[VvcCabacDumpSymbol]) -> Self {
        let mut categories = Self::default();
        for symbol in symbols {
            categories.add_symbol(*symbol);
        }
        categories
    }

    fn add_symbol(&mut self, symbol: VvcCabacDumpSymbol) {
        let bits = vvc_cabac_symbol_bin_count(symbol);
        match vvc_cabac_symbol_category(symbol) {
            VvcCtuBitCategory::Partition => self.partition_bits += bits,
            VvcCtuBitCategory::LumaMode => self.luma_mode_bits += bits,
            VvcCtuBitCategory::ChromaMode => self.chroma_mode_bits += bits,
            VvcCtuBitCategory::Residual => self.residual_bits += bits,
            VvcCtuBitCategory::Intrabc => self.intrabc_bits += bits,
            VvcCtuBitCategory::Inter => self.inter_bits += bits,
            VvcCtuBitCategory::Palette => self.palette_bits += bits,
            VvcCtuBitCategory::Other => self.other_bits += bits,
        }
    }
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcCtuBitCategory {
    Partition,
    LumaMode,
    ChromaMode,
    Residual,
    Intrabc,
    Inter,
    Palette,
    Other,
}

#[cfg(feature = "vvc-stats")]
fn vvc_cabac_symbol_bin_count(symbol: VvcCabacDumpSymbol) -> usize {
    match symbol.kind {
        VvcCabacDumpSymbol::BIN_EP
        | VvcCabacDumpSymbol::BIN_TRM
        | VvcCabacDumpSymbol::BIN_CTX
        | VvcCabacDumpSymbol::BIN_CTX_DIRECT => 1,
        VvcCabacDumpSymbol::BINS_EP => (symbol.data & 0x3f) as usize,
        _ => 0,
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_cabac_symbol_category(symbol: VvcCabacDumpSymbol) -> VvcCtuBitCategory {
    match symbol.kind {
        VvcCabacDumpSymbol::BIN_CTX => {
            vvc_context_id_bit_category(((symbol.data >> 8) & 0x03ff) as u16)
        }
        // The residual path's bypass payload is dominated by coefficient signs
        // and remainders. Mode-index bypass bins are comparatively small, so
        // the category is a useful residual-pressure proxy rather than an
        // exact arithmetic-coded bit attribution.
        VvcCabacDumpSymbol::BIN_EP | VvcCabacDumpSymbol::BINS_EP => VvcCtuBitCategory::Residual,
        VvcCabacDumpSymbol::BIN_TRM => VvcCtuBitCategory::Other,
        _ => VvcCtuBitCategory::Other,
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_context_id_bit_category(ctx_id: u16) -> VvcCtuBitCategory {
    match ctx_id {
        0..=3 | 19 | 20 | 24..=41 => VvcCtuBitCategory::Partition,
        4 | 21 | 53 | 305..=310 => VvcCtuBitCategory::LumaMode,
        13 | 14 | 304 => VvcCtuBitCategory::ChromaMode,
        42..=52 => VvcCtuBitCategory::Palette,
        274..=282 => VvcCtuBitCategory::Intrabc,
        265..=273 | 283..=294 | 314 => VvcCtuBitCategory::Inter,
        5..=12 | 15..=18 | 22 | 23 | 54..=70 | 71..=264 | 295..=303 | 311..=313 => {
            VvcCtuBitCategory::Residual
        }
        _ => VvcCtuBitCategory::Other,
    }
}

#[cfg(feature = "vvc-stats")]
impl VvcCtuBitSink {
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            sink: JsonlInstrumentationSink::append_from_env(VVC_CTU_BITS_ENV)
                .map_err(|err| err.to_string())?,
            frame_idx: None,
            frame_state: None,
        })
    }

    fn is_enabled(&self) -> bool {
        self.sink.is_some()
    }

    fn write_ctu(
        &mut self,
        frame_idx: usize,
        picture_geometry: VvcVideoGeometry,
        region: VvcCtuRegion,
        format: VvcPictureFormat,
        lossless: bool,
        slice_qp: i32,
        chroma_qp: i32,
        quantized: &VvcQuantizedColor,
        luma_max_leaf_size: u16,
        slice_config: VvcSliceSyntaxConfig,
    ) -> Result<(), String> {
        let Some(sink) = self.sink.as_mut() else {
            return Ok(());
        };
        let Some(params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
            region.geometry,
            quantized.clone(),
            luma_max_leaf_size,
            slice_config.coding_tree.chroma_sampling,
            slice_config.coding_tree.dual_tree_intra,
        ) else {
            return Ok(());
        };
        if self.frame_idx != Some(frame_idx) {
            self.frame_idx = Some(frame_idx);
            self.frame_state = Some(VvcFrameCtuCabacState::new(
                picture_geometry,
                slice_config,
                slice_config.inter_enabled && frame_idx > 0,
            ));
        }
        let frame_state = self
            .frame_state
            .as_mut()
            .expect("VVC CTU bit sink must initialize frame CABAC state");
        let dump = vvc_ctu_partition_cabac_dump_with_frame_state(
            frame_state,
            region.slice_address,
            &params,
            slice_config,
        );
        let luma_modes = vvc_luma_mode_counts(quantized);
        let chroma_modes = vvc_chroma_mode_counts(quantized);
        let residual_coding = vvc_tu_residual_coding_counts(quantized);
        let bdpcm = vvc_tu_bdpcm_counts(quantized);
        let energy = quantized.residual_energy_stats;
        let bit_categories = VvcCtuBitCategories::from_symbols(&dump.semantic_symbols);
        let search = quantized.intra_search_stats;
        let line = format!(
            "{{\"codec\":\"vvc\",\"source\":\"framefinery\",\"path\":\"residual_ctu\",\"frame_index\":{},\"ctu_address\":{},\"sb_x\":{},\"sb_y\":{},\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"superblock_size\":{},\"chroma_sampling\":\"{:?}\",\"bit_depth\":{},\"lossless\":{},\"slice_qp\":{},\"chroma_qp\":{},\"luma_tu_count\":{},\"chroma_tu_count\":{},\"luma_tu_transform_skip_count\":{},\"luma_tu_transformed_count\":{},\"cb_tu_transform_skip_count\":{},\"cb_tu_transformed_count\":{},\"cr_tu_transform_skip_count\":{},\"cr_tu_transformed_count\":{},\"chroma_tu_transform_skip_count\":{},\"chroma_tu_transformed_count\":{},\"luma_bdpcm_horizontal_count\":{},\"luma_bdpcm_vertical_count\":{},\"chroma_bdpcm_horizontal_count\":{},\"chroma_bdpcm_vertical_count\":{},\"luma_residual_sse_total\":{},\"luma_residual_sse_coded_first4x4\":{},\"luma_residual_sse_uncoded_tail\":{},\"chroma_residual_sse_total\":{},\"chroma_residual_sse_coded_first4x4\":{},\"chroma_residual_sse_uncoded_tail\":{},\"luma_candidate_count\":{},\"luma_candidate_dc\":{},\"luma_candidate_planar\":{},\"luma_candidate_directional\":{},\"luma_candidate_directional_coarse\":{},\"luma_candidate_directional_refinement\":{},\"luma_rd_refinement_attempts\":{},\"luma_rd_refinement_switches\":{},\"chroma_candidate_count\":{},\"chroma_candidate_derived\":{},\"chroma_candidate_explicit\":{},\"chroma_candidate_cclm\":{},\"chroma_candidate_cclm_linear\":{},\"chroma_candidate_mdlm_left\":{},\"chroma_candidate_mdlm_top\":{},\"chroma_rd_refinement_attempts\":{},\"chroma_rd_refinement_switches\":{},\"luma_mode_dc\":{},\"luma_mode_planar\":{},\"luma_mode_horizontal\":{},\"luma_mode_vertical\":{},\"luma_mode_angular\":{},\"chroma_mode_derived\":{},\"chroma_mode_dc\":{},\"chroma_mode_planar\":{},\"chroma_mode_horizontal\":{},\"chroma_mode_vertical\":{},\"chroma_mode_angular\":{},\"chroma_mode_cclm\":{},\"chroma_mode_cclm_linear\":{},\"chroma_mode_mdlm_left\":{},\"chroma_mode_mdlm_top\":{},\"partition_bits\":{},\"luma_mode_bits\":{},\"chroma_mode_bits\":{},\"residual_bits\":{},\"intrabc_bits\":{},\"inter_bits\":{},\"palette_bits\":{},\"other_bits\":{},\"context_bins\":{},\"semantic_symbols\":{},\"bin_engine_events\":{},\"total_symbol_bits\":{}}}",
            frame_idx,
            region.slice_address,
            region.origin_x / VVC_CTU_SIZE,
            region.origin_y / VVC_CTU_SIZE,
            region.origin_x,
            region.origin_y,
            region.geometry.width,
            region.geometry.height,
            VVC_CTU_SIZE,
            format.chroma_sampling,
            format.bit_depth.bits(),
            lossless,
            slice_qp,
            chroma_qp,
            quantized.luma_tu_count,
            quantized.chroma_tu_count,
            residual_coding.luma_transform_skip,
            residual_coding.luma_transformed,
            residual_coding.cb_transform_skip,
            residual_coding.cb_transformed,
            residual_coding.cr_transform_skip,
            residual_coding.cr_transformed,
            residual_coding.chroma_transform_skip(),
            residual_coding.chroma_transformed(),
            bdpcm.luma_horizontal,
            bdpcm.luma_vertical,
            bdpcm.chroma_horizontal,
            bdpcm.chroma_vertical,
            energy.luma_total_sse,
            energy.luma_coded_first4x4_sse,
            energy.luma_uncoded_tail_sse,
            energy.chroma_total_sse,
            energy.chroma_coded_first4x4_sse,
            energy.chroma_uncoded_tail_sse,
            search.luma_candidates(),
            search.luma_dc_candidates,
            search.luma_planar_candidates,
            search.luma_directional_candidates(),
            search.luma_directional_coarse_candidates,
            search.luma_directional_refinement_candidates,
            search.luma_rd_refinement_attempts,
            search.luma_rd_refinement_switches,
            search.chroma_candidates(),
            search.chroma_derived_candidates,
            search.chroma_explicit_candidates,
            search.chroma_cclm_candidates,
            search.chroma_cclm_linear_candidates,
            search.chroma_cclm_mdlm_left_candidates,
            search.chroma_cclm_mdlm_top_candidates,
            search.chroma_rd_refinement_attempts,
            search.chroma_rd_refinement_switches,
            luma_modes.dc,
            luma_modes.planar,
            luma_modes.horizontal,
            luma_modes.vertical,
            luma_modes.angular,
            chroma_modes.derived,
            chroma_modes.dc,
            chroma_modes.planar,
            chroma_modes.horizontal,
            chroma_modes.vertical,
            chroma_modes.angular,
            chroma_modes.cclm,
            chroma_modes.cclm_linear,
            chroma_modes.mdlm_left,
            chroma_modes.mdlm_top,
            bit_categories.partition_bits,
            bit_categories.luma_mode_bits,
            bit_categories.chroma_mode_bits,
            bit_categories.residual_bits,
            bit_categories.intrabc_bits,
            bit_categories.inter_bits,
            bit_categories.palette_bits,
            bit_categories.other_bits,
            dump.context_bin_count,
            dump.semantic_symbols.len(),
            dump.bin_engine_events.len(),
            dump.bits.len(),
        );
        sink.write_json_line(&line).map_err(|err| err.to_string())?;
        sink.flush().map_err(|err| err.to_string())
    }
}
