impl<'a> Av2LossySubsampledTileState<'a> {
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
}
