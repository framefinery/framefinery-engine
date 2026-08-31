struct Av2TxbSkipSyntax {
    all_zero_name: &'static str,
    nonzero_name: &'static str,
    static_cdf_key: usize,
    cdf: [u16; 6],
}

impl Av2TxbSkipSyntax {
    fn write(self, writer: &mut Av2EntropyWriter, all_zero: bool) {
        let name = if all_zero {
            self.all_zero_name
        } else {
            self.nonzero_name
        };
        let mut cdf = self.cdf;
        writer.write_symbol_with_static_cdf_key(
            name,
            self.static_cdf_key,
            usize::from(all_zero),
            &mut cdf,
            2,
            false,
        );
    }
}

fn write_y_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_txb_skip(writer, skip_ctx, true);
}

fn write_y_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_txb_skip(writer, skip_ctx, false);
}

fn write_y_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 luma TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx1",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx2",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx3",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx4",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_TX4X4_CTX5_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: y_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_y_inter_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_inter_txb_skip(writer, skip_ctx, true);
}

fn write_y_inter_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_y_inter_txb_skip(writer, skip_ctx, false);
}

fn write_y_inter_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 1, 5, 5, "AV2 inter luma TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        1 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx1",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx1",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx2",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx2",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx3",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx3",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx4",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx4",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
        _ => (
            "tile.coeff.y.inter_txb_all_zero_tx4x4_ctx5",
            "tile.coeff.y.inter_txb_nonzero_tx4x4_ctx5",
            DEFAULT_TXB_SKIP_Y_INTER_TX4X4_CTX5_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: y_inter_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_y_fsc_txb_all_zero(writer: &mut Av2EntropyWriter) {
    write_y_fsc_txb_skip(writer, true);
}

fn write_y_fsc_txb_nonzero(writer: &mut Av2EntropyWriter) {
    write_y_fsc_txb_skip(writer, false);
}

fn write_y_fsc_txb_skip(writer: &mut Av2EntropyWriter, all_zero: bool) {
    Av2TxbSkipSyntax {
        all_zero_name: "tile.coeff.y.txb_all_zero_fsc_tx4x4_ctx9",
        nonzero_name: "tile.coeff.y.txb_nonzero_fsc_tx4x4_ctx9",
        static_cdf_key: y_fsc_txb_skip_static_cdf_key(9),
        cdf: DEFAULT_TXB_SKIP_Y_FSC_TX4X4_CTX9_CDF,
    }
    .write(writer, all_zero);
}

fn write_u_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    write_u_txb_skip(writer, skip_ctx, use_fsc, true);
}

fn write_u_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool) {
    write_u_txb_skip(writer, skip_ctx, use_fsc, false);
}

fn write_u_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        6 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx6",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX6_CDF,
        ),
        6 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx6",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx6",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX6_CDF,
        ),
        7 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx7",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX7_CDF,
        ),
        7 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx7",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx7",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX7_CDF,
        ),
        8 if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        8 => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
        _ if use_fsc => (
            "tile.coeff.u.txb_all_zero_fsc_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_fsc_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_FSC_TX4X4_CTX8_CDF,
        ),
        _ => (
            "tile.coeff.u.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.u.txb_nonzero_tx4x4_ctx8",
            DEFAULT_TXB_SKIP_U_TX4X4_CTX8_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: u_txb_skip_static_cdf_key(skip_ctx, use_fsc),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_v_txb_all_zero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_v_txb_skip(writer, skip_ctx, true);
}

fn write_v_txb_nonzero(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    write_v_txb_skip(writer, skip_ctx, false);
}

fn write_v_txb_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
    let skip_ctx = normalize_av2_context(skip_ctx, 0, 11, 11, "AV2 V TXB skip");
    let (all_zero_name, nonzero_name, cdf) = match skip_ctx {
        0 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx0",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx0",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX0_CDF,
        ),
        1 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx1",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx1",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX1_CDF,
        ),
        2 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx2",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx2",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX2_CDF,
        ),
        3 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx3",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx3",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX3_CDF,
        ),
        4 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx4",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx4",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX4_CDF,
        ),
        5 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx5",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx5",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX5_CDF,
        ),
        6 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx6",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx6",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX6_CDF,
        ),
        7 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx7",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx7",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX7_CDF,
        ),
        8 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx8",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx8",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX8_CDF,
        ),
        9 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx9",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx9",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX9_CDF,
        ),
        10 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx10",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx10",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX10_CDF,
        ),
        11 => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
        _ => (
            "tile.coeff.v.txb_all_zero_tx4x4_ctx11",
            "tile.coeff.v.txb_nonzero_tx4x4_ctx11",
            DEFAULT_V_TXB_SKIP_TX4X4_CTX11_CDF,
        ),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: v_txb_skip_static_cdf_key(skip_ctx),
        cdf,
    }
    .write(writer, all_zero);
}

fn write_u_txb_all_zero_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
) {
    write_u_txb_skip_tx8x8(writer, skip_ctx, use_inter_contexts, true);
}

fn write_u_txb_nonzero_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
) {
    write_u_txb_skip_tx8x8(writer, skip_ctx, use_inter_contexts, false);
}

fn write_u_txb_skip_tx8x8(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    use_inter_contexts: bool,
    all_zero: bool,
) {
    let skip_ctx = normalize_av2_context(skip_ctx, 6, 8, 8, "AV2 U TXB skip 8x8");
    let (all_zero_name, nonzero_name, cdf) = match (use_inter_contexts, skip_ctx) {
        (false, 6) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx6",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx6",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX6_CDF,
        ),
        (false, 7) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx7",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx7",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX7_CDF,
        ),
        (false, 8) => (
            "tile.coeff.u.txb_all_zero_tx8x8_ctx8",
            "tile.coeff.u.txb_nonzero_tx8x8_ctx8",
            DEFAULT_TXB_SKIP_U_TX8X8_CTX8_CDF,
        ),
        (true, 6) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx6",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx6",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX6_CDF,
        ),
        (true, 7) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx7",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx7",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX7_CDF,
        ),
        (true, 8) => (
            "tile.coeff.u.txb_all_zero_inter_tx8x8_ctx8",
            "tile.coeff.u.txb_nonzero_inter_tx8x8_ctx8",
            DEFAULT_TXB_SKIP_U_INTER_TX8X8_CTX8_CDF,
        ),
        _ => unreachable!("normalized AV2 U TXB skip 8x8 context is in range"),
    };
    Av2TxbSkipSyntax {
        all_zero_name,
        nonzero_name,
        static_cdf_key: u_txb_skip_tx8x8_static_cdf_key(skip_ctx, use_inter_contexts),
        cdf,
    }
    .write(writer, all_zero);
}

#[cfg(test)]
mod txb_skip_writer_tests {
    use super::*;

    fn assert_skip_pair(
        write_all_zero: impl FnOnce(&mut Av2EntropyWriter),
        write_nonzero: impl FnOnce(&mut Av2EntropyWriter),
        expected_all_zero_name: &str,
        expected_nonzero_name: &str,
    ) {
        let mut all_zero_writer = Av2EntropyWriter::with_cdf_updates(true);
        write_all_zero(&mut all_zero_writer);
        let all_zero = all_zero_writer.finish();
        assert_eq!(all_zero.fields.len(), 1);
        assert_eq!(all_zero.fields[0].name, expected_all_zero_name);
        assert_eq!(all_zero.fields[0].symbol, Some(1));

        let mut nonzero_writer = Av2EntropyWriter::with_cdf_updates(true);
        write_nonzero(&mut nonzero_writer);
        let nonzero = nonzero_writer.finish();
        assert_eq!(nonzero.fields.len(), 1);
        assert_eq!(nonzero.fields[0].name, expected_nonzero_name);
        assert_eq!(nonzero.fields[0].symbol, Some(0));
    }

    #[test]
    fn txb_skip_writer_pairs_preserve_all_context_names_and_symbols() {
        for skip_ctx in 1..=5 {
            assert_skip_pair(
                |writer| write_y_txb_all_zero(writer, skip_ctx),
                |writer| write_y_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.y.txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.y.txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
            assert_skip_pair(
                |writer| write_y_inter_txb_all_zero(writer, skip_ctx),
                |writer| write_y_inter_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.y.inter_txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.y.inter_txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
        }

        assert_skip_pair(
            write_y_fsc_txb_all_zero,
            write_y_fsc_txb_nonzero,
            "tile.coeff.y.txb_all_zero_fsc_tx4x4_ctx9",
            "tile.coeff.y.txb_nonzero_fsc_tx4x4_ctx9",
        );

        for skip_ctx in 6..=8 {
            for use_fsc in [false, true] {
                let fsc = if use_fsc { "_fsc" } else { "" };
                assert_skip_pair(
                    |writer| write_u_txb_all_zero(writer, skip_ctx, use_fsc),
                    |writer| write_u_txb_nonzero(writer, skip_ctx, use_fsc),
                    &format!("tile.coeff.u.txb_all_zero{fsc}_tx4x4_ctx{skip_ctx}"),
                    &format!("tile.coeff.u.txb_nonzero{fsc}_tx4x4_ctx{skip_ctx}"),
                );
            }
            for use_inter_contexts in [false, true] {
                let inter = if use_inter_contexts { "_inter" } else { "" };
                assert_skip_pair(
                    |writer| write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts),
                    |writer| write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts),
                    &format!("tile.coeff.u.txb_all_zero{inter}_tx8x8_ctx{skip_ctx}"),
                    &format!("tile.coeff.u.txb_nonzero{inter}_tx8x8_ctx{skip_ctx}"),
                );
            }
        }

        for skip_ctx in 0..=11 {
            assert_skip_pair(
                |writer| write_v_txb_all_zero(writer, skip_ctx),
                |writer| write_v_txb_nonzero(writer, skip_ctx),
                &format!("tile.coeff.v.txb_all_zero_tx4x4_ctx{skip_ctx}"),
                &format!("tile.coeff.v.txb_nonzero_tx4x4_ctx{skip_ctx}"),
            );
        }
    }
}
