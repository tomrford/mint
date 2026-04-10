use mint_cli::commands;

#[path = "common/mod.rs"]
mod common;

#[test]
fn smoke_build_examples_all_formats_and_options() {
    common::ensure_out_dir();

    let layouts = ["tests/data/blocks.toml", "tests/data/blocks.yaml"];
    let blocks = ["block", "block2", "block3"];

    for layout_path in layouts {
        let Some(ds) = common::find_working_datasource() else {
            continue;
        };

        let cfg = mint_cli::layout::load_layout(layout_path).expect("layout loads");

        for &blk in &blocks {
            if !cfg.blocks.contains_key(blk) {
                continue;
            }

            // Hex
            let args_hex =
                common::build_args(layout_path, blk, mint_cli::output::args::OutputFormat::Hex);
            commands::build(&args_hex, Some(ds.as_ref())).expect("build hex");
            common::assert_out_file_exists(&args_hex.output.out);

            // Mot
            let args_mot =
                common::build_args(layout_path, blk, mint_cli::output::args::OutputFormat::Mot);
            commands::build(&args_mot, Some(ds.as_ref())).expect("build mot");
            common::assert_out_file_exists(&args_mot.output.out);
        }

        let block_inputs = cfg
            .blocks
            .keys()
            .map(|name| mint_cli::layout::args::BlockNames {
                name: name.clone(),
                file: layout_path.to_string(),
            })
            .collect::<Vec<_>>();

        if !block_inputs.is_empty() {
            let args_combined = common::build_args_for_layouts(
                block_inputs,
                mint_cli::output::args::OutputFormat::Hex,
                "combined",
            );

            commands::build(&args_combined, Some(ds.as_ref())).expect("build combined");
            common::assert_out_file_exists(&args_combined.output.out);
        }
    }
}
