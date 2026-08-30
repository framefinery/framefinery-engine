fn chroma_directional_angle_for_mode(mode: Av2LosslessSubsampledModeDecision) -> Option<i16> {
    if let Some((base, delta)) = mode.luma_intra_mode.directional() {
        if base.chroma_mode() == mode.chroma_intra_mode {
            return Some(base.angle(delta));
        }
    }
    av2_chroma_directional_angle(mode.chroma_intra_mode)
}

#[derive(Debug, Clone, Copy, Default)]
struct Av2DcHvBdpcmTxbScores {
    dc: usize,
    horizontal: usize,
    vertical: usize,
    bdpcm_horizontal: usize,
    bdpcm_vertical: usize,
}

impl Av2DcHvBdpcmTxbScores {
    fn add_assign(&mut self, other: Self) {
        self.dc += other.dc;
        self.horizontal += other.horizontal;
        self.vertical += other.vertical;
        self.bdpcm_horizontal += other.bdpcm_horizontal;
        self.bdpcm_vertical += other.bdpcm_vertical;
    }
}

fn residual_sample_proxy_magnitude_scale(kind: Av2CoefficientProxyKind) -> usize {
    match kind {
        Av2CoefficientProxyKind::LumaIdtx => 4,
        Av2CoefficientProxyKind::LumaTransform => 4,
        Av2CoefficientProxyKind::ChromaTransform => 3,
    }
}

fn add_residual_sample_proxy_score(score: &mut usize, delta: i32, magnitude_scale: usize) {
    let level = delta.unsigned_abs() as usize;
    if level != 0 {
        *score += 80 + level.min(255) * magnitude_scale;
    }
}
