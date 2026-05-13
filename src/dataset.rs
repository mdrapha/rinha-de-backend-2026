use half::f16;
use serde::Deserialize;
use std::io::{BufReader, BufWriter, Read, Write};

use crate::distance::DIMS;
use crate::vptree::RefEntry;

#[derive(Deserialize)]
struct RawReference {
    vector: Vec<f64>,
    label: String,
}

pub struct Index {
    pub vectors: Vec<f16>,
    pub labels: Vec<u8>,
    pub medians: Vec<f32>,
    pub n: usize,
}

pub fn preprocess(gz_path: &str, out_path: &str) {
    eprintln!("Loading references from {}...", gz_path);

    let file = std::fs::File::open(gz_path).expect("Failed to open gz file");
    let decoder = flate2::read::GzDecoder::new(file);
    let reader = BufReader::with_capacity(4 * 1024 * 1024, decoder);
    let entries: Vec<RawReference> =
        serde_json::from_reader(reader).expect("Failed to parse references JSON");

    eprintln!("Loaded {} references, converting to f16...", entries.len());

    let n = entries.len();
    let mut ref_entries: Vec<RefEntry> = entries
        .iter()
        .map(|e| {
            let mut vector = [f16::ZERO; DIMS];
            for i in 0..DIMS {
                vector[i] = f16::from_f64(e.vector[i]);
            }
            RefEntry {
                vector,
                label: e.label == "fraud",
            }
        })
        .collect();

    drop(entries);

    eprintln!("Building VP-tree for {} vectors...", n);
    let mut medians = vec![0.0f32; n];
    crate::vptree::build_vptree(&mut ref_entries, &mut medians);

    eprintln!("Writing index to {}...", out_path);
    write_index(out_path, &ref_entries, &medians);
    eprintln!("Done. Index size: {} bytes", 8 + n * DIMS * 2 + n + n * 4);
}

fn write_index(path: &str, entries: &[RefEntry], medians: &[f32]) {
    let n = entries.len();
    let file = std::fs::File::create(path).expect("Failed to create index file");
    let mut w = BufWriter::with_capacity(1024 * 1024, file);

    w.write_all(&(n as u32).to_le_bytes()).unwrap();
    w.write_all(&(DIMS as u32).to_le_bytes()).unwrap();

    for entry in entries {
        let bytes = unsafe {
            std::slice::from_raw_parts(entry.vector.as_ptr() as *const u8, DIMS * 2)
        };
        w.write_all(bytes).unwrap();
    }

    for entry in entries {
        w.write_all(&[entry.label as u8]).unwrap();
    }

    for &m in medians {
        w.write_all(&m.to_le_bytes()).unwrap();
    }

    w.flush().unwrap();
}

pub fn load_index(path: &str) -> Index {
    eprintln!("Loading index from {}...", path);

    let file = std::fs::File::open(path).expect("Failed to open index file");
    let mut reader = BufReader::with_capacity(1024 * 1024, file);

    let mut header = [0u8; 8];
    reader.read_exact(&mut header).unwrap();
    let n = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let dims = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    assert_eq!(dims, DIMS, "Index dimensions mismatch");

    let vectors_len = n * dims;
    let mut vectors = vec![f16::ZERO; vectors_len];
    let vectors_bytes =
        unsafe { std::slice::from_raw_parts_mut(vectors.as_mut_ptr() as *mut u8, vectors_len * 2) };
    reader.read_exact(vectors_bytes).unwrap();

    let mut labels = vec![0u8; n];
    reader.read_exact(&mut labels).unwrap();

    let mut medians = vec![0.0f32; n];
    let medians_bytes =
        unsafe { std::slice::from_raw_parts_mut(medians.as_mut_ptr() as *mut u8, n * 4) };
    reader.read_exact(medians_bytes).unwrap();

    eprintln!("Index loaded: {} vectors, {} dims", n, dims);

    Index {
        vectors,
        labels,
        medians,
        n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vptree;

    #[test]
    fn round_trip_index() {
        let mut entries = Vec::new();
        for i in 0..10 {
            let mut vector = [f16::ZERO; DIMS];
            vector[0] = f16::from_f32(i as f32 * 0.1);
            entries.push(RefEntry {
                vector,
                label: i >= 7,
            });
        }

        let n = entries.len();
        let mut medians = vec![0.0f32; n];
        vptree::build_vptree(&mut entries, &mut medians);

        let dir = std::env::temp_dir();
        let path = dir.join("test_index.bin");
        let path_str = path.to_str().unwrap();

        write_index(path_str, &entries, &medians);
        let index = load_index(path_str);

        assert_eq!(index.n, n);
        assert_eq!(index.labels.len(), n);
        assert_eq!(index.vectors.len(), n * DIMS);
        assert_eq!(index.medians.len(), n);

        for i in 0..n {
            assert_eq!(index.labels[i], entries[i].label as u8);
            for d in 0..DIMS {
                assert_eq!(
                    index.vectors[i * DIMS + d].to_bits(),
                    entries[i].vector[d].to_bits()
                );
            }
            assert!((index.medians[i] - medians[i]).abs() < 1e-6);
        }

        std::fs::remove_file(path_str).ok();
    }
}
