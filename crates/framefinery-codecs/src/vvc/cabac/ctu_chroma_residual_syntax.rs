impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_chroma_residual(
        contexts: &mut VvcCabacContexts,
        slice_config: VvcSliceSyntaxConfig,
        chroma_sampling: ChromaSampling,
        cabac: &mut VvcCabacEncoder,
        component: VvcResidualComponent,
        node: VvcCodingTreeNode,
        dc_level: i16,
        ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
        has_ac: bool,
        transform_skip: bool,
        bdpcm: bool,
    ) {
        let width = usize::from(vvc_chroma_width(node, chroma_sampling));
        let height = usize::from(vvc_chroma_height(node, chroma_sampling));
        let log2_width = (width as u16).ilog2() as u8;
        let log2_height = (height as u16).ilog2() as u8;
        let mut residual = VvcResidualCabacEncoder::new(contexts, slice_config.residual_options());
        VvcResidualCabacSymbolStream::emit_chroma_stored_coefficients(
            component,
            log2_width,
            log2_height,
            dc_level,
            ac_levels,
            has_ac,
            transform_skip,
            bdpcm,
            &mut residual,
            cabac,
        );
    }
}
