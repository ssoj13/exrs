//! Benchmark for deep data reading - compares parallel vs sequential performance.

use std::time::Instant;
use std::path::Path;
use exr::image::read::deep::read_deep;

fn benchmark_file(path: &str, parallel: bool) -> Option<(String, usize, u128)> {
    if !Path::new(path).exists() {
        return None;
    }
    
    let name = Path::new(path).file_name()?.to_str()?.to_string();
    
    let start = Instant::now();
    
    let mut reader = read_deep()
        .all_channels()
        .first_valid_layer()
        .all_attributes();
    
    if !parallel {
        reader = reader.non_parallel();
    }
    
    let image = reader.from_file(path).ok()?;
    let elapsed = start.elapsed().as_millis();
    
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
    
    println!("\n=== Deep Data Read Benchmark: Parallel vs Sequential ===\n");
    println!("{:<30} {:>10} {:>10} {:>10} {:>8}", "File", "Samples", "Seq(ms)", "Par(ms)", "Speedup");
    println!("{}", "-".repeat(75));
    
    for path in &files {
        // Sequential
        let seq = benchmark_file(path, false);
        // Parallel
        let par = benchmark_file(path, true);
        
        if let (Some((name, samples, seq_ms)), Some((_, _, par_ms))) = (seq, par) {
            let speedup = if par_ms > 0 { seq_ms as f64 / par_ms as f64 } else { 0.0 };
            println!("{:<30} {:>10} {:>10} {:>10} {:>8.2}x", name, samples, seq_ms, par_ms, speedup);
        }
    }
    
    println!("\nNote: Parallel decompression uses rayon thread pool.");
    println!("Speedup depends on compression ratio and CPU cores.");
}
