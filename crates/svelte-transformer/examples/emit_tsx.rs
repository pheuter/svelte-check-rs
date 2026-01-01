use std::fs;
use svelte_parser::parse;
use svelte_transformer::{transform, TransformOptions};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: emit_tsx <file.svelte>");
    let source = fs::read_to_string(&path).expect("Failed to read file");
    let parsed = parse(&source);
    let result = transform(
        &parsed.document,
        TransformOptions {
            filename: Some(path.clone()),
            source_maps: true,
        },
    );
    println!("{}", result.tsx_code);
}
