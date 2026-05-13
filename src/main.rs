mod dataset;
mod distance;
mod server;
mod types;
mod vectorize;
mod vptree;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "preprocess" {
        let gz_path = args
            .get(2)
            .expect("Usage: rinha preprocess <references.json.gz> <output.bin>");
        let out_path = args
            .get(3)
            .expect("Usage: rinha preprocess <references.json.gz> <output.bin>");
        dataset::preprocess(gz_path, out_path);
        return;
    }

    let index_path =
        std::env::var("INDEX_PATH").unwrap_or_else(|_| "/data/index.bin".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    rt.block_on(server::run(&index_path, port));
}
