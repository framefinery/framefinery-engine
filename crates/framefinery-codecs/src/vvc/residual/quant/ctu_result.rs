struct VvcCtuQuantizationResult {
    luma_metadata: VvcLumaTuMetadata,
    chroma_metadata: VvcChromaTuMetadata,
    luma_tu_count: usize,
    chroma_tu_count: usize,
    #[cfg(feature = "vvc-stats")]
    intra_search_stats: VvcIntraSearchStats,
    #[cfg(feature = "vvc-stats")]
    residual_energy_stats: VvcResidualEnergyStats,
}

impl VvcCtuQuantizationResult {
    fn into_quantized_color(self, source_frame: &VvcSampledFrame) -> VvcQuantizedColor {
        let color = source_frame.sampled_color();
        let cb_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
            color.u,
            source_frame.format.bit_depth,
        ));
        let cr_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
            color.v,
            source_frame.format.bit_depth,
        ));
        let VvcLumaTuMetadata {
            luma_tu_intra_modes,
            luma_tu_remainders,
            luma_tu_negative,
            luma_tu_dc_levels,
            luma_tu_ac_levels,
            luma_tu_has_ac,
            luma_tu_scc_decisions,
            luma_tu_transform_skip,
            luma_tu_bdpcm_modes,
            luma_tu_mrl_index,
            luma_tu_mts_index,
        } = self.luma_metadata;
        let VvcChromaTuMetadata {
            chroma_tu_intra_modes,
            cb_tu_dc_levels,
            cr_tu_dc_levels,
            cb_tu_ac_levels,
            cr_tu_ac_levels,
            cb_tu_has_ac,
            cr_tu_has_ac,
            cb_tu_transform_skip,
            cr_tu_transform_skip,
            chroma_tu_bdpcm_modes,
        } = self.chroma_metadata;

        VvcQuantizedColor {
            y: vvc_downshift_sample_to_u8(color.y, source_frame.format.bit_depth),
            u: finalized_vvc_chroma_sample(
                cb_tu_transform_skip.first().copied().unwrap_or(false),
                color.u,
                cb_rem,
                source_frame.format.bit_depth,
            ),
            v: finalized_vvc_chroma_sample(
                cr_tu_transform_skip.first().copied().unwrap_or(false),
                color.v,
                cr_rem,
                source_frame.format.bit_depth,
            ),
            luma_tu_intra_modes,
            luma_tu_remainders,
            luma_tu_negative,
            luma_tu_dc_levels,
            luma_tu_ac_levels,
            luma_tu_has_ac,
            luma_tu_scc_decisions,
            luma_tu_transform_skip,
            luma_tu_bdpcm_modes,
            luma_tu_mrl_index,
            luma_tu_mts_index,
            luma_tu_count: self.luma_tu_count,
            chroma_tu_count: self.chroma_tu_count,
            chroma_tu_intra_modes,
            cb_tu_dc_levels,
            cr_tu_dc_levels,
            cb_tu_ac_levels,
            cr_tu_ac_levels,
            cb_tu_has_ac,
            cr_tu_has_ac,
            cb_tu_transform_skip,
            cr_tu_transform_skip,
            chroma_tu_bdpcm_modes,
            cb_rem,
            cr_rem,
            #[cfg(feature = "vvc-stats")]
            intra_search_stats: self.intra_search_stats,
            #[cfg(feature = "vvc-stats")]
            residual_energy_stats: self.residual_energy_stats,
        }
    }
}

fn finalized_vvc_chroma_sample(
    transform_skip: bool,
    source: VvcSample,
    quantized_remainder: u8,
    bit_depth: SampleBitDepth,
) -> u8 {
    if transform_skip {
        vvc_downshift_sample_to_u8(source, bit_depth)
    } else {
        reconstruct_vvc_chroma(quantized_remainder)
    }
}
