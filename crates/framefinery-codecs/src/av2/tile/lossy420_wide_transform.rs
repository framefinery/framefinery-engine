impl<'a> Av2LossySubsampledTileState<'a> {
    fn regular_dct_quantized_tx8x8_residual_candidate(
        &self,
        analysis: &Av2LossyTx8x8Analysis,
    ) -> Av2LossyTx8x8QuantizedResidualCandidate {
        if analysis.residual.iter().all(|&residual| residual == 0) {
            // An exact predictor match has an all-zero regular-DCT result;
            // preserve the ordinary candidate shape without running FDCT or
            // IDCT on the 8x8 block.
            return Av2LossyTx8x8QuantizedResidualCandidate {
                coefficients: [0; TX8X8_SAMPLES],
                residual: [0; TX8X8_SAMPLES],
            };
        }
        let coefficients = av2_fdct8x8(&analysis.residual);
        let (qcoeff, dqcoeff) =
            av2_regular_quantize_dct8x8_with_params(&coefficients, self.regular_quant_params);
        let residual = if dqcoeff[1..]
            .iter()
            .all(|&coefficient| coefficient == 0)
        {
            av2_idct8x8_dc_only(&dqcoeff, self.bit_depth)
        } else {
            av2_idct8x8(&dqcoeff, self.bit_depth)
        };
        Av2LossyTx8x8QuantizedResidualCandidate {
            coefficients: av2_regular_quantized_level_coefficients_tx8x8(&qcoeff),
            residual,
        }
    }

    fn regular_dct_quantized_tx4x8_residual_candidate(
        &self,
        analysis: &Av2LossyTx8x8Analysis,
    ) -> Av2LossyTx4x8QuantizedResidualCandidate {
        debug_assert!(analysis.visible_width <= TX4X8_WIDTH);
        debug_assert!(analysis.visible_height <= TX4X8_HEIGHT);
        if analysis.residual.iter().all(|&residual| residual == 0) {
            // The unused right-hand columns are already zero, so an exact
            // predictor match also has an all-zero TX_4X8 candidate.
            return Av2LossyTx4x8QuantizedResidualCandidate {
                coefficients: [0; TX4X8_SAMPLES],
                residual: [0; TX4X8_SAMPLES],
            };
        }
        let mut residual = [0i32; TX4X8_SAMPLES];
        for local_y in 0..TX4X8_HEIGHT {
            for local_x in 0..TX4X8_WIDTH {
                residual[local_y * TX4X8_WIDTH + local_x] =
                    analysis.residual[local_y * TX8X8_SIZE + local_x];
            }
        }
        let coefficients = av2_fdct4x8(&residual);
        let (qcoeff, dqcoeff) =
            av2_regular_quantize_dct4x8_with_params(&coefficients, self.regular_quant_params);
        Av2LossyTx4x8QuantizedResidualCandidate {
            coefficients: av2_regular_quantized_level_coefficients_tx4x8(&qcoeff),
            residual: av2_idct4x8(&dqcoeff, self.bit_depth),
        }
    }
}
