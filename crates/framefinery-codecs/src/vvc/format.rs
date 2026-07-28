#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcPictureFormat {
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCodingTreeConfig {
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
}

impl VvcCodingTreeConfig {
    const fn yuv(chroma_sampling: ChromaSampling) -> Self {
        Self {
            chroma_sampling,
            dual_tree_intra: true,
        }
    }

    const fn single_tree_444() -> Self {
        Self {
            chroma_sampling: ChromaSampling::Cs444,
            dual_tree_intra: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcVuiSignal {
    progressive_source: bool,
    interlaced_source: bool,
    non_packed: bool,
    non_projected: bool,
    colour_primaries: u8,
    transfer_characteristics: u8,
    matrix_coeffs: u8,
    full_range: bool,
}

impl VvcVuiSignal {
    const fn srgb_gbr_compatible() -> Self {
        Self {
            progressive_source: true,
            interlaced_source: false,
            non_packed: true,
            non_projected: true,
            colour_primaries: 1,
            transfer_characteristics: 13,
            // H.266/VTM forbid identity matrix coefficients for 4:4:4 VUI.
            // Keep the colour volume explicit while leaving the RGB matrix
            // unspecified until a compatible VVC RGB signalling path is added.
            matrix_coeffs: 2,
            full_range: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcSyntaxToolFlags {
    ibc_enabled: bool,
    palette_enabled: bool,
    transform_skip_enabled: bool,
    bdpcm_enabled: bool,
    mts_enabled: bool,
    explicit_mts_intra_enabled: bool,
    lfnst_enabled: bool,
    isp_enabled: bool,
    mip_enabled: bool,
    joint_cbcr_enabled: bool,
    mrl_enabled: bool,
    cclm_enabled: bool,
    dependent_quantization_enabled: bool,
    sign_data_hiding_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcSliceSyntaxConfig {
    coding_tree: VvcCodingTreeConfig,
    tools: VvcSyntaxToolFlags,
    ref_pic_resampling_enabled: bool,
    entry_point_offsets_present: bool,
    slice_qp: i32,
    vui_signal: Option<VvcVuiSignal>,
}

impl VvcSyntaxToolFlags {
    const fn residual(
        chroma_sampling: ChromaSampling,
        residual_mode: VvcResidualCodingMode,
    ) -> Self {
        Self {
            ibc_enabled: false,
            palette_enabled: false,
            transform_skip_enabled: true,
            bdpcm_enabled: true,
            mts_enabled: false,
            explicit_mts_intra_enabled: !residual_mode.is_lossless(),
            lfnst_enabled: false,
            isp_enabled: false,
            mip_enabled: false,
            joint_cbcr_enabled: false,
            mrl_enabled: true,
            cclm_enabled: true,
            dependent_quantization_enabled: false,
            sign_data_hiding_enabled: false,
        }
        .without_unsupported_chroma_tools(chroma_sampling)
    }

    const fn without_unsupported_chroma_tools(self, _chroma_sampling: ChromaSampling) -> Self {
        self
    }

    const fn palette_444() -> Self {
        Self {
            ibc_enabled: true,
            palette_enabled: true,
            transform_skip_enabled: true,
            bdpcm_enabled: true,
            mts_enabled: false,
            explicit_mts_intra_enabled: false,
            lfnst_enabled: false,
            isp_enabled: false,
            mip_enabled: false,
            joint_cbcr_enabled: false,
            mrl_enabled: false,
            cclm_enabled: false,
            dependent_quantization_enabled: false,
            sign_data_hiding_enabled: false,
        }
    }

    const fn mts_enabled(self) -> bool {
        self.mts_enabled || self.explicit_mts_intra_enabled
    }
}

impl VvcSliceSyntaxConfig {
    const fn new(coding_tree: VvcCodingTreeConfig, tools: VvcSyntaxToolFlags) -> Self {
        Self {
            coding_tree,
            tools,
            ref_pic_resampling_enabled: true,
            entry_point_offsets_present: true,
            slice_qp: 32,
            vui_signal: None,
        }
    }

    #[cfg(test)]
    const fn yuv420_residual() -> Self {
        Self::residual_lossy(ChromaSampling::Cs420)
    }

    #[cfg(test)]
    const fn residual_lossy(chroma_sampling: ChromaSampling) -> Self {
        Self::residual(chroma_sampling, VvcResidualCodingMode::Lossy)
    }

    #[cfg(test)]
    fn residual_lossless(chroma_sampling: ChromaSampling, bit_depth: SampleBitDepth) -> Self {
        let mut config = Self::residual(chroma_sampling, VvcResidualCodingMode::Lossless);
        config.slice_qp = vvc_lossless_slice_qp(bit_depth);
        config
    }

    const fn residual(
        chroma_sampling: ChromaSampling,
        residual_mode: VvcResidualCodingMode,
    ) -> Self {
        Self::new(
            VvcCodingTreeConfig::yuv(chroma_sampling),
            VvcSyntaxToolFlags::residual(chroma_sampling, residual_mode),
        )
    }

    const fn palette_444() -> Self {
        let mut config = Self::new(
            VvcCodingTreeConfig::single_tree_444(),
            VvcSyntaxToolFlags::palette_444(),
        );
        config.slice_qp = VVC_PALETTE_DEFAULT_SLICE_QP;
        config
    }

    const fn without_lossless_speed_unused_tools(mut self) -> Self {
        self.tools.mrl_enabled = false;
        self.tools.cclm_enabled = false;
        self
    }

    #[cfg(test)]
    const fn palette_444_lossless(bit_depth: SampleBitDepth) -> Self {
        let mut config = Self::palette_444();
        config.slice_qp = vvc_palette_lossless_slice_qp(bit_depth);
        config
    }

    const fn for_picture_format(format: VvcPictureFormat) -> Self {
        Self::residual(format.chroma_sampling, VvcResidualCodingMode::Lossy)
    }

    const fn with_vui_signal(mut self, vui_signal: VvcVuiSignal) -> Self {
        self.vui_signal = Some(vui_signal);
        self
    }

    const fn residual_options(self) -> VvcResidualCabacOptions {
        VvcResidualCabacOptions {
            transform_skip_enabled: self.tools.transform_skip_enabled,
            explicit_mts_intra_enabled: self.tools.explicit_mts_intra_enabled,
            dependent_quantization_enabled: self.tools.dependent_quantization_enabled,
            sign_data_hiding_enabled: self.tools.sign_data_hiding_enabled,
            lfnst_enabled: self.tools.lfnst_enabled,
            sbt_enabled: false,
        }
    }
}
