#[cfg(test)]
fn append_palette_syntax_token_bits(bits: &mut Vec<bool>, token: VvcPaletteSyntaxToken) {
    match token.kind {
        VvcPaletteSyntaxTokenKind::Eg0 { value } => append_eg0_bits(bits, value),
        VvcPaletteSyntaxTokenKind::FixedLength { value, bit_count } => {
            append_fixed_bits(bits, value as u64, bit_count);
        }
    }
}

fn append_palette_syntax_token_cabac(cabac: &mut VvcCabacEncoder, token: VvcPaletteSyntaxToken) {
    match token.kind {
        VvcPaletteSyntaxTokenKind::Eg0 { value } => {
            vvc_encode_exp_golomb_ep_combined(cabac, value, 0)
        }
        VvcPaletteSyntaxTokenKind::FixedLength { value, bit_count } => {
            cabac.encode_bins_ep(value, bit_count as u32);
        }
    }
}

fn encode_trunc_bin_code_ep(cabac: &mut VvcCabacEncoder, symbol: u32, num_symbols: u32) {
    debug_assert!(symbol < num_symbols);
    let thresh = 31 - num_symbols.leading_zeros();
    let val = 1 << thresh;
    let b = num_symbols - val;
    if symbol < val - b {
        cabac.encode_bins_ep(symbol, thresh);
    } else {
        cabac.encode_bins_ep(symbol + val - b, thresh + 1);
    }
}

#[cfg(test)]
fn append_eg0_bits(bits: &mut Vec<bool>, value: u32) {
    let code_num = value + 1;
    let bit_count = 32 - code_num.leading_zeros();
    for _ in 0..bit_count - 1 {
        bits.push(false);
    }
    for bit in (0..bit_count).rev() {
        bits.push(((code_num >> bit) & 1) != 0);
    }
}

#[cfg(test)]
fn append_fixed_bits(bits: &mut Vec<bool>, value: u64, bit_count: u8) {
    for bit in (0..bit_count).rev() {
        bits.push(((value >> bit) & 1) != 0);
    }
}
