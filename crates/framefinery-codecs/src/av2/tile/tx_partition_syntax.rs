fn write_regular_q_tx_partition_split_8x8(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
) {
    write_regular_q_tx_partition_split_8x8_with_cdfs(
        writer,
        decision,
        "tile.tx_partition.do_partition",
        AV2_STATIC_CDF_TX_PARTITION_INTRA_DO_8X8,
        DEFAULT_TXFM_DO_PARTITION_INTRA_8X8_CDF,
        "tile.tx_partition.partition_type",
        AV2_STATIC_CDF_TX_PARTITION_INTRA_TYPE_8X8,
        DEFAULT_TXFM_4WAY_PARTITION_INTRA_8X8_CDF,
    );
}

fn write_regular_q_inter_tx_partition_split_8x8(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
) {
    write_regular_q_tx_partition_split_8x8_with_cdfs(
        writer,
        decision,
        "tile.inter_tx_partition.do_partition",
        AV2_STATIC_CDF_TX_PARTITION_INTER_DO_8X8,
        DEFAULT_TXFM_DO_PARTITION_INTER_8X8_CDF,
        "tile.inter_tx_partition.partition_type",
        AV2_STATIC_CDF_TX_PARTITION_INTER_TYPE_8X8,
        DEFAULT_TXFM_4WAY_PARTITION_INTER_8X8_CDF,
    );
}

fn write_regular_q_tx_partition_split_8x8_with_cdfs(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    do_partition_name: &'static str,
    do_partition_static_key: usize,
    do_partition_cdf: [u16; 6],
    partition_type_name: &'static str,
    partition_type_static_key: usize,
    partition_type_cdf: [u16; 11],
) {
    debug_assert_eq!(decision.block_size.width, 8);
    debug_assert_eq!(decision.block_size.height, 8);

    let mut do_partition_cdf = do_partition_cdf;
    writer.write_symbol_with_static_cdf_key(
        do_partition_name,
        do_partition_static_key,
        1,
        &mut do_partition_cdf,
        2,
        false,
    );
    let mut partition_type_cdf = partition_type_cdf;
    writer.write_symbol_with_static_cdf_key(
        partition_type_name,
        partition_type_static_key,
        0,
        &mut partition_type_cdf,
        7,
        false,
    );
}
