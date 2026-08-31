impl<'a> Av2LossySubsampledTileState<'a> {
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
            av2_regular_quantize_with_params(&coefficients, self.regular_quant_params);
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
        let residual = if dqcoeff[1..].iter().all(|&coefficient| coefficient == 0) {
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
}
