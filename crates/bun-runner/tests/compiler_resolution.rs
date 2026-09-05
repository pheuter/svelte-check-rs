use bun_runner::{BunCompileOptions, BunInput, BunRunner};
use camino::{Utf8Path, Utf8PathBuf};
use std::fs;

fn install_compiler(root: &Utf8Path, version: &str, message: &str, broken: bool) {
    let package = root.join("node_modules/svelte");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("package.json"),
        serde_json::json!({
            "name": "svelte", "version": version, "type": "module",
            "exports": { "./compiler": "./compiler.js", "./package.json": "./package.json" }
        })
        .to_string(),
    )
    .unwrap();
    let source = if broken {
        "throw new Error('broken local compiler');".to_string()
    } else {
        format!("export async function preprocess(code) {{ return {{ code }}; }}; export function compile() {{ return {{ warnings: [{{code:'test_warning', message:{}, start:{{line:1,column:0}},end:{{line:1,column:1}}}}] }}; }}", serde_json::to_string(message).unwrap())
    };
    fs::write(package.join("compiler.js"), source).unwrap();
}

#[tokio::test]
async fn per_file_compilers_invalidate_cache_and_preserve_loader_failures() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let nested = root.join("packages/nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.join("package.json"), "{\"type\":\"module\"}").unwrap();
    install_compiler(&root, "5.0.0", "root", false);
    install_compiler(&nested, "5.1.0", "nested-v1", false);
    let bun = BunRunner::ensure_bun(Some(&root)).await.unwrap();
    let runner = BunRunner::new(bun, root.clone(), 1).unwrap();
    let inputs: Vec<_> = [root.join("Root.svelte"), nested.join("Nested.svelte")]
        .into_iter()
        .map(|filename| BunInput {
            filename,
            source: "<p>unchanged source</p>".into(),
            options: BunCompileOptions::default(),
        })
        .collect();
    let diagnostics = runner.check_files(inputs.clone()).await.unwrap();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        ["root", "nested-v1"]
    );

    install_compiler(&nested, "5.2.0", "nested-v2", false);
    assert_eq!(
        runner.check_files(inputs.clone()).await.unwrap()[1].message,
        "nested-v2"
    );
    // Same-version edits must invalidate the actual compiler fingerprint too.
    install_compiler(&nested, "5.2.0", "nested-edited", false);
    assert_eq!(
        runner.check_files(inputs.clone()).await.unwrap()[1].message,
        "nested-edited"
    );
    fs::remove_dir_all(nested.join("node_modules/svelte")).unwrap();
    assert_eq!(
        runner.check_files(inputs.clone()).await.unwrap()[1].message,
        "root"
    );
    install_compiler(&nested, "5.3.0", "nested-added", false);
    assert_eq!(
        runner.check_files(inputs.clone()).await.unwrap()[1].message,
        "nested-added"
    );

    install_compiler(&nested, "5.3.0", "", true);
    let error = runner.check_files(inputs.clone()).await.unwrap_err();
    assert!(error.to_string().contains("broken local compiler"));
    let error = runner
        .preprocess_files(vec![inputs[1].clone()], None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("broken local compiler"));

    fs::remove_dir_all(root.join("node_modules/svelte")).unwrap();
    install_compiler(&nested, "5.4.0", "nested-only", false);
    assert_eq!(
        runner.check_files(vec![inputs[1].clone()]).await.unwrap()[0].message,
        "nested-only"
    );
    fs::remove_dir_all(nested.join("node_modules/svelte")).unwrap();
    assert!(runner.check_files(inputs).await.is_err());
}
