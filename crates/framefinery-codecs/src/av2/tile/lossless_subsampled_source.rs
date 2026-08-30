impl<'a> Av2LosslessSubsampledTileState<'a> {
    fn source_backed_luma_intra_residual4x4(
        &self,
        x0: usize,
        y0: usize,
        mode: Av2LumaIntraMode,
    ) -> Option<[i32; TX4X4_SAMPLES]> {
        self.source_backed_chroma_intra_residual4x4(
            Av2LosslessPlane::Y,
            x0,
            y0,
            chroma_mode_for_luma_mode(mode),
        )
    }

    fn source_backed_chroma_intra_residual4x4(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2ChromaIntraMode,
    ) -> Option<[i32; TX4X4_SAMPLES]> {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let source = self.source_block4x4(plane, x0, y0);
        match mode {
            Av2ChromaIntraMode::Dc => {
                let predictor = i32::from(self.source_backed_dc_predictor(plane, x0, y0));
                for index in 0..TX4X4_SAMPLES {
                    residual[index] = source[index] - predictor;
                }
                Some(residual)
            }
            Av2ChromaIntraMode::Horizontal => {
                for local_y in 0..TX4X4_SIZE {
                    let predictor =
                        i32::from(self.source_backed_h_predictor(plane, x0, y0, local_y));
                    let row_start = local_y * TX4X4_SIZE;
                    for local_x in 0..TX4X4_SIZE {
                        let pos = row_start + local_x;
                        residual[pos] = source[pos] - predictor;
                    }
                }
                Some(residual)
            }
            Av2ChromaIntraMode::Vertical => {
                let mut predictors = [0i32; TX4X4_SIZE];
                for (local_x, predictor) in predictors.iter_mut().enumerate() {
                    *predictor = i32::from(self.source_backed_v_predictor(plane, x0, y0, local_x));
                }
                for local_y in 0..TX4X4_SIZE {
                    for local_x in 0..TX4X4_SIZE {
                        let pos = local_y * TX4X4_SIZE + local_x;
                        residual[pos] = source[pos] - predictors[local_x];
                    }
                }
                Some(residual)
            }
            _ => None,
        }
    }

    fn source_backed_dc_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.dc_predictor_with(plane, x0, y0, &edge_sample)
    }

    fn source_backed_h_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.h_predictor_with(plane, x0, y0, local_y, &edge_sample)
    }

    fn source_backed_v_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.v_predictor_with(plane, x0, y0, local_x, &edge_sample)
    }
}
