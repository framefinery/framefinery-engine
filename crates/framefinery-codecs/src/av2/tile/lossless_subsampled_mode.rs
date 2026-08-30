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
        match plane {
            Av2LosslessPlane::Y => self.luma_residual4x4_for_mode(
                x0,
                y0,
                mode,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
            ),
            Av2LosslessPlane::U | Av2LosslessPlane::V => self.chroma_residual4x4_for_mode(
                plane,
                x0,
                y0,
                mode,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
            ),
        }
    }

    fn luma_residual4x4_for_mode(
        &self,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if let Some(horz) = mode.luma_bdpcm_horz {
            return self.dpcm_residual4x4(Av2LosslessPlane::Y, x0, y0, horz);
        }
        if self.source_backed_recon && !mode.use_fsc {
            if let Some(residual) =
                self.source_backed_luma_intra_residual4x4(x0, y0, mode.luma_intra_mode)
            {
                return residual;
            }
        }
        self.luma_intra_residual4x4(
            x0,
            y0,
            mode.luma_intra_mode,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
        )
    }

    fn chroma_residual4x4_for_mode(
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
        if mode.chroma_use_bdpcm {
            let horizontal = mode.chroma_intra_mode.is_horizontal();
            return self.dpcm_residual4x4(plane, x0, y0, horizontal);
        }
        if self.source_backed_recon && !mode.use_fsc {
            if let Some(residual) = self.source_backed_chroma_intra_residual4x4(
                plane,
                x0,
                y0,
                mode.chroma_intra_mode,
            ) {
                return residual;
            }
        }
        self.intra_residual4x4(
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
        )
    }
}
