//! Benchmark for deep data reading - measures sequential vs potential parallel performance.

use std::time::Instant;
use std::path::Path;
use exr::image::read::deep::read_first_deep_layer_from_file;

fn benchmark_file(path: &str) -> Option<(String, usize, u128)> {
    if !Path::new(path).exists() {
        return None;
    }
    
    let name = Path::new(path).file_name()?.to_str()?.to_string();
    
    let start = Instant::now();
    let image = read_first_deep_layer_from_file(path).ok()?;
    let elapsed = start.elapsed().as_millis();
    
    // Samples are stored in first channel's sample_data
    let total_samples = image.layer_data.channel_data.list[0].sample_data.total_samples();
    
    Some((name, total_samples, elapsed))
}

#[test]
fn benchmark_deep_read() {
    let files = [
        "tests/images/valid/openexr/v2/LowResLeftView/Balls.exr",
        "tests/images/valid/openexr/v2/LowResLeftView/Ground.exr",
        "tests/images/valid/openexr/v2/deep_large/MiniCooper720p.exr",
        "tests/images/valid/openexr/v2/deep_large/Teaset720p.exr",
        "tests/images/valid/openexr/v2/deep_large/PiranhnaAlienRun720p.exr",
    ];
    
    println!("\n=== Deep Data Read Benchmark ===\n");
    println!("{:<30} {:>12} {:>10} {:>12}", "File", "Samples", "Time(ms)", "Samples/ms");
    println!("{}", "-".repeat(70));
    
    for path in &files {
        if let Some((name, samples, ms)) = benchmark_file(path) {
            let rate = if ms > 0 { samples as f64 / ms as f64 } else { 0.0 };
            println!("{:<30} {:>12} {:>10} {:>12.0}", name, samples, ms, rate);
        }
    }
    
    println!("\nNote: Current implementation is sequential.");
    println!("Parallel decompression could significantly improve performance on large files.");
}
