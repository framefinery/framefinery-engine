impl<'a> Av2LosslessSubsampledTileState<'a> {
    fn tx4x4_residual_for_mode(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        self.tx4x4_residual_for_mode_with_reference(
            plane,
            x0,
            y0,
            mode,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            Av2LosslessIntraReference::Reconstructed,
        )
    }

    fn tx4x4_residual_for_mode_with_reference(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        reference: Av2LosslessIntraReference,
    ) -> [i32; TX4X4_SAMPLES] {
        match plane {
            Av2LosslessPlane::Y => {
                if let Some(horz) = mode.luma_bdpcm_horz {
                    return self.dpcm_residual4x4_with_reference(
                        plane, x0, y0, horz, reference,
                    );
                }
                self.luma_intra_residual4x4_with_reference(
                    x0,
                    y0,
                    mode.luma_intra_mode,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    reference,
                )
            }
            Av2LosslessPlane::U | Av2LosslessPlane::V => {
                if mode.chroma_use_bdpcm {
                    return self.dpcm_residual4x4_with_reference(
                        plane,
                        x0,
                        y0,
                        mode.chroma_intra_mode.is_horizontal(),
                        reference,
                    );
                }
                self.intra_residual4x4_with_reference(
                    plane,
                    x0,
                    y0,
                    mode.chroma_intra_mode,
                    chroma_directional_angle_for_mode(mode),
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    reference,
                )
            }
        }
    }
}
