pub(in crate::vvc) fn vvc_encode_exp_golomb_ep_combined(
    cabac: &mut super::VvcCabacEncoder,
    mut symbol: u32,
    mut count: u32,
) {
    let mut bins = 0;
    let mut num_bins = 0;
    while symbol >= (1 << count) {
        bins <<= 1;
        bins += 1;
        num_bins += 1;
        symbol -= 1 << count;
        count += 1;
    }
    bins <<= 1;
    num_bins += 1;
    cabac.encode_bins_ep((bins << count) | symbol, num_bins + count);
}
