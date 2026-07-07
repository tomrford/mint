use mint_cli::commands;

#[path = "common/mod.rs"]
mod common;

#[test]
fn smoke_build_examples_all_formats_and_options() {
    common::ensure_out_dir();

    let layouts = ["../mint-core/tests/data/blocks.toml"];
    let blocks = ["block", "block2", "block3"];

    for layout_path in layouts {
        let ds = common::find_working_datasource();

        let cfg = mint_core::layout::load_layout(layout_path).expect("layout loads");

        for &blk in &blocks {
            if !cfg.blocks.contains_key(blk) {
                continue;
            }

            // Hex
            let args_hex =
                common::build_args(layout_path, blk, mint_core::output::OutputFormat::Hex);
            commands::build(&args_hex, Some(ds.as_ref())).expect("build hex");
            common::assert_out_file_exists(&args_hex.output.out);

            // Mot
            let args_mot =
                common::build_args(layout_path, blk, mint_core::output::OutputFormat::Mot);
            commands::build(&args_mot, Some(ds.as_ref())).expect("build mot");
            common::assert_out_file_exists(&args_mot.output.out);
        }

        let block_inputs = cfg
            .blocks
            .keys()
            .map(|name| mint_core::build::BlockSelector::named(layout_path, name))
            .collect::<Vec<_>>();

        if !block_inputs.is_empty() {
            let args_combined = common::build_args_for_layouts(
                block_inputs,
                mint_core::output::OutputFormat::Hex,
                "combined",
            );

            commands::build(&args_combined, Some(ds.as_ref())).expect("build combined");
            common::assert_out_file_exists(&args_combined.output.out);
        }
    }
}
